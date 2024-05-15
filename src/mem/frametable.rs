use core::{
    ptr,
    slice::{from_raw_parts, from_raw_parts_mut},
};

use alloc::{collections::BTreeMap, vec::Vec};

use crate::{
    fs::disk::DISKFS,
    io::{Read, Write},
    mem::{
        in_kernel_space,
        palloc::{self, UserPool},
        PG_SIZE,
    },
    sync::{Intr, Lazy, Mutex},
    thread::Manager,
};

use super::{PTEFlags, PhysAddr};

#[derive(Default, Clone, Copy)]
pub struct FrameInfo {
    /// thread that refers to this frame
    thread: isize,
    /// virtual address referred to by the user thread
    v_addr: usize,
    /// if dirty and evicted, then resides in a swap file?
    swap: bool,
    /// whether this frame is in use
    active: bool,
}

pub struct SupplementInfo {
    swap: bool,
    location: usize,
}

pub struct TableInner {
    frames: Vec<FrameInfo>,
    clock_hand: usize,
}

pub struct FrameTable(Lazy<Mutex<TableInner, Intr>>);

/// Supplemental table. Indexed by the thread id and the virtual address.
pub struct SupplementTable(Lazy<Mutex<BTreeMap<(usize, usize), SupplementInfo>, Intr>>);

struct SwapInner {
    location: usize,
}

impl From<usize> for SwapInner {
    fn from(value: usize) -> Self {
        Self { location: value }
    }
}

pub struct SwapTable(Lazy<Mutex<BTreeMap<(usize, usize), SwapInner>, Intr>>);

unsafe impl Sync for FrameTable {}
unsafe impl Sync for SupplementTable {}
unsafe impl Sync for SwapTable {}

impl FrameTable {
    /// Allocates a userpage. This function will not map v_addr to result address. v_addr is only provided for frame info.
    pub unsafe fn alloc_page(thread: isize, v_addr: usize, swap: bool, flags: PTEFlags) -> usize {
        assert!(v_addr % PG_SIZE == 0);
        let mut table = Self::instance().lock();
        let result = if let Some(addr) = UserPool::alloc_pages(1) {
            addr
        } else {
            // memory exhausted. try to evict an existing page
            let ptr = Self::evict() as *mut u8;
            // zero out
            ptr::write_bytes(ptr, 0, PG_SIZE);
            ptr
        } as usize;

        // setup frame table
        let lowest = PhysAddr::from(UserPool::lowest()).ppn();
        kprintln!(
            "lowest: {:#x}, result: {:#x}",
            lowest,
            PhysAddr::from(result).ppn()
        );
        let index = PhysAddr::from(result).ppn() - lowest;
        assert!(table.frames[index].active == false);
        table.frames[index] = FrameInfo {
            thread,
            v_addr,
            swap,
            active: true,
        };

        assert!(in_kernel_space(result));
        result
    }

    pub unsafe fn dealloc_page(ptr: usize) {
        assert!(ptr % PG_SIZE == 0);
        let lowest = PhysAddr::from_pa(UserPool::lowest()).ppn();
        let index = PhysAddr::from(ptr).ppn() - lowest;
        let mut table = Self::instance().lock();
        table.frames[index].active = false;
        UserPool::dealloc_pages(ptr as *mut u8, 1);
    }

    pub unsafe fn dealloc_pages(ptr: usize, n: usize) {
        for i in 0..n {
            Self::dealloc_page(ptr + i * PG_SIZE);
        }
    }

    /// This function tries to find an appropriate page for replacement. It returns
    /// the kernel virtual address of the in-memory frame
    /// Clock algorithm is adopted, so this function will only be invoked during
    /// pagefault or alloc_pages
    /// - We use 'use' bit to record whether this page is referenced recently
    /// - When this function is called, advance the clock hand, then check the use bit
    /// -   1. if use bit = 1 then clear use bit and left it alone
    /// -   2. if use bit = 0 then select the page as replacement candidate
    /// this function will not modify the candidate page
    unsafe fn evict() -> usize {
        let mut table = Self::instance().lock();
        loop {
            table.clock_hand = (table.clock_hand + 1) % palloc::USER_POOL_LIMIT;
            let mut info = table.frames[table.clock_hand];
            assert!(info.active);

            let thread = Manager::get().get_by_id(info.thread).unwrap();
            let pt = match &thread.pagetable {
                Some(pt) => pt.lock(),
                None => unreachable!(),
            };
            let entry = pt.get_pte_mut(info.v_addr).unwrap();

            if entry.is_accessed() {
                entry.clean_access_bit();
            } else {
                if entry.is_dirty() {
                    todo!()
                }

                entry.clean_valid_bit();
                info.active = false;
                return entry.pa().into_va();
            }
        }
    }

    pub fn instance() -> &'static Mutex<TableInner, Intr> {
        static TABLE: FrameTable = FrameTable(Lazy::new(|| {
            Mutex::new(TableInner {
                frames: alloc::vec![FrameInfo::default(); palloc::USER_POOL_LIMIT],
                clock_hand: 0,
            })
        }));
        &TABLE.0
    }
}

impl SupplementTable {
    fn put(thread: usize, ptr: usize, info: SupplementInfo) {
        assert!(ptr & PG_SIZE == 0);
        Self::instance().lock().insert((thread, ptr), info);
    }

    fn instance() -> &'static Mutex<BTreeMap<(usize, usize), SupplementInfo>, Intr> {
        static TABLE: SupplementTable = SupplementTable(Lazy::new(|| Mutex::new(BTreeMap::new())));
        &TABLE.0
    }
}

impl SwapTable {
    /// writes the page at ptr in kernel virtual address to disk, returns its disk location
    pub unsafe fn store_page(thread: usize, ptr: usize) -> usize {
        assert!(ptr % PG_SIZE == 0);
        let mut file = DISKFS.get().alloc_page().unwrap();
        file.write(from_raw_parts(ptr as *const u8, PG_SIZE))
            .unwrap();
        Self::instance()
            .lock()
            .insert((thread, ptr), file.ino().into());
        file.ino()
    }

    /// load the page which resided in swap section to memory referred by ptr in kernel virtual address
    unsafe fn load_to_frame(ptr: usize, location: usize) {
        assert!(ptr % PG_SIZE == 0);
        let mut file = DISKFS.get().from_location(location).unwrap();
        let buf = from_raw_parts_mut(ptr as *mut u8, PG_SIZE);
        let result = file.read(buf).unwrap();
        DISKFS.get().free_location(location);
        assert!(result == PG_SIZE)
    }

    /// load one page from swap file to the memory
    pub unsafe fn load_page(thread: usize, ptr: usize) {
        assert!(ptr % PG_SIZE == 0);
        let inner = Self::instance().lock().remove(&(thread, ptr)).unwrap();
        Self::load_to_frame(ptr, inner.location);
    }

    fn instance() -> &'static Mutex<BTreeMap<(usize, usize), SwapInner>, Intr> {
        static TABLE: SwapTable = SwapTable(Lazy::new(|| Mutex::new(BTreeMap::new())));
        &TABLE.0
    }
}

/// moves page referred by ptr back to the memory
pub fn demand_page(ptr: usize) {
    assert!(ptr % PG_SIZE == 0);
    assert!(in_kernel_space(ptr));
    todo!()
}
