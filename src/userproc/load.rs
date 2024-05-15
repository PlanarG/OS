use alloc::vec;
use elf_rs::{Elf, ElfFile, ProgramHeaderEntry, ProgramHeaderFlags, ProgramType};

use crate::fs::File;
use crate::io::prelude::*;
use crate::mem::pagetable::{PTEFlags, PageTable};

use crate::mem::{div_round_up, FrameTable, PageAlign, PhysAddr, PG_MASK, PG_SIZE};
use crate::{OsError, Result};

#[derive(Debug, Clone, Copy)]
pub(super) struct ExecInfo {
    pub entry_point: usize,
    pub init_sp: usize,
}

/// Loads an executable file
///
/// ## Params
/// - `pagetable`: User's pagetable. We install the mapping to executable codes into it.
///
/// ## Return
/// On success, returns `Ok(usize, usize)`:
/// - arg0: the entry point of user program
/// - arg1: the initial sp of user program
pub(super) fn load_executable(
    file: &mut File,
    pagetable: &mut PageTable,
    thread: isize,
) -> Result<(ExecInfo, *mut u8)> {
    let exec_info = load_elf(file, pagetable, thread)?;

    // Initialize user stack.
    let stack_va = init_user_stack(pagetable, exec_info.init_sp, thread);

    // Forbid modifying executable file when running
    file.deny_write();

    Ok((exec_info, stack_va))
}

/// Parses the specified executable file and loads segments
fn load_elf(file: &mut File, pagetable: &mut PageTable, thread: isize) -> Result<ExecInfo> {
    // Ensure cursor is at the beginning
    file.rewind()?;

    let len = file.len()?;
    let mut buf = vec![0u8; len];
    file.read(&mut buf)?;

    let elf = match Elf::from_bytes(&buf) {
        Ok(Elf::Elf64(elf)) => elf,
        Ok(Elf::Elf32(_)) | Err(_) => return Err(OsError::UnknownFormat),
    };

    // load each loadable segment into memory
    elf.program_header_iter()
        .filter(|p| p.ph_type() == ProgramType::LOAD)
        .for_each(|p| load_segment(&buf, &p, pagetable, thread));

    Ok(ExecInfo {
        entry_point: elf.elf_header().entry_point() as _,
        init_sp: 0x80500000,
    })
}

/// Loads one segment and installs pagetable mappings
fn load_segment(
    filebuf: &[u8],
    phdr: &ProgramHeaderEntry,
    pagetable: &mut PageTable,
    thread: isize,
) {
    assert_eq!(phdr.ph_type(), ProgramType::LOAD);

    // Meaningful contents of this segment starts from `fileoff`.
    let fileoff = phdr.offset() as usize;
    // But we will read and install from `read_pos`.
    let mut readpos = fileoff & !PG_MASK;

    // Install flags.
    let mut leaf_flag = PTEFlags::V | PTEFlags::U | PTEFlags::R;
    if phdr.flags().contains(ProgramHeaderFlags::EXECUTE) {
        leaf_flag |= PTEFlags::X;
    }
    if phdr.flags().contains(ProgramHeaderFlags::WRITE) {
        leaf_flag |= PTEFlags::W;
    }

    // Install position: `ubase`.
    let ubase = (phdr.vaddr() as usize) & !PG_MASK;
    let pageoff = (phdr.vaddr() as usize) & PG_MASK;
    assert_eq!(fileoff & PG_MASK, pageoff);

    // How many pages need to be allocated
    let pages = div_round_up(pageoff + phdr.memsz() as usize, PG_SIZE);
    let mut readbytes = phdr.filesz() as usize + pageoff;

    // Allocate & map pages
    for p in 0..pages {
        let readsz = readbytes.min(PG_SIZE);
        let uaddr = ubase + p * PG_SIZE;

        let buf = unsafe { FrameTable::alloc_page(thread, uaddr, true, leaf_flag) };
        let page = unsafe { (buf as *mut [u8; PG_SIZE]).as_mut().unwrap() };

        page[..readsz].copy_from_slice(&filebuf[readpos..readpos + readsz]);
        pagetable.map(buf.into(), uaddr, 1, leaf_flag);

        readbytes -= readsz;
        readpos += readsz;
    }

    assert_eq!(readbytes, 0);
}

/// Initializes the user stack.
/// stack_va is required to locate the stack we're going to modify, since
/// we can't use init_sp directly. stack_page is not activated yet.
fn init_user_stack(pagetable: &mut PageTable, init_sp: usize, thread: isize) -> *mut u8 {
    assert!(init_sp % PG_SIZE == 0, "initial sp address misaligns");

    let stack_page_begin = PageAlign::floor(init_sp - 1);
    let flags = PTEFlags::V | PTEFlags::R | PTEFlags::W | PTEFlags::U;

    // Allocate a page from UserPool as user stack.
    let stack_va = unsafe { FrameTable::alloc_page(thread, stack_page_begin, true, flags) };
    let stack_pa = PhysAddr::from(stack_va);

    // Get the start address of stack page

    // Install mapping
    pagetable.map(stack_pa, stack_page_begin, PG_SIZE, flags);

    #[cfg(feature = "debug")]
    kprintln!(
        "[USERPROC] User Stack Mapping: (k){:p} -> (u) {:#x}",
        stack_va,
        stack_page_begin
    );

    // Now stack_va points to the bottom of this newly allowcated page
    // Adjust it to the top of this page
    (stack_va as usize + PG_SIZE) as *mut u8
}
