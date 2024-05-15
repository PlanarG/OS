use crate::mem::userbuf::{
    __knrl_read_usr_byte, __knrl_read_usr_exit, __knrl_write_usr_byte, __knrl_write_usr_exit,
};
use crate::mem::{FrameTable, KernelPgTable, PTEFlags, PageAlign, PhysAddr, PG_SIZE};
use crate::thread::{self, Mutex};
use crate::trap::Frame;
use crate::userproc;

use riscv::register::scause::Exception::{self, *};
use riscv::register::sstatus::{self, SPP};

pub fn handler(frame: &mut Frame, fault: Exception, addr: usize) {
    let privilege = frame.sstatus.spp();
    let sp = frame.x[2];

    let current = thread::current();

    // does this fault trigger a stack growth?
    if addr == sp && fault == StorePageFault {
        let mut table = match &current.pagetable {
            Some(table) => table.lock(),
            None => unreachable!(),
        };
        let flags = PTEFlags::V | PTEFlags::R | PTEFlags::W | PTEFlags::U;
        let page_begin = sp.floor();
        let stack_va = unsafe { FrameTable::alloc_page(current.id(), page_begin, true, flags) };
        table.map(PhysAddr::from(stack_va), page_begin, PG_SIZE, flags);
        return;
    }

    let present = {
        let pt = current.pagetable.as_ref().map(Mutex::lock);
        let table = pt.as_deref().unwrap_or(KernelPgTable::get());
        match table.get_pte(addr) {
            Some(entry) => entry.is_valid(),
            None => false,
        }
    };

    unsafe { sstatus::set_sie() };

    kprintln!(
        "Page fault at {:#x}: {} error {} page in {} context.",
        addr,
        if present { "rights" } else { "not present" },
        match fault {
            StorePageFault => "writing",
            LoadPageFault => "reading",
            InstructionPageFault => "fetching instruction",
            _ => panic!("Unknown Page Fault"),
        },
        match privilege {
            SPP::Supervisor => "kernel",
            SPP::User => "user",
        }
    );

    match privilege {
        SPP::Supervisor => {
            if frame.sepc == __knrl_read_usr_byte as _ {
                // Failed to read user byte from kernel space when trap in pagefault
                frame.x[11] = 1; // set a1 to non-zero
                frame.sepc = __knrl_read_usr_exit as _;
            } else if frame.sepc == __knrl_write_usr_byte as _ {
                // Failed to write user byte from kernel space when trap in pagefault
                frame.x[11] = 1; // set a1 to non-zero
                frame.sepc = __knrl_write_usr_exit as _;
            } else {
                panic!("Kernel page fault");
            }
        }
        SPP::User => {
            kprintln!(
                "User thread {} dying due to page fault.",
                thread::current().name()
            );
            userproc::exit(-1);
        }
    }
}
