//! User process.
//!

mod load;

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::asm;
use core::mem::MaybeUninit;
use core::ptr::copy_nonoverlapping;
use riscv::register::sstatus;

use crate::fs::File;
use crate::mem::pagetable::KernelPgTable;
use crate::thread::{self, current, schedule, ChildStatus, Thread};
use crate::trap::{trap_exit_u, Frame};

pub struct UserProc {
    #[allow(dead_code)]
    bin: File,
    pub parent: Arc<Thread>,
}

impl UserProc {
    pub fn new(file: File) -> Self {
        Self {
            bin: file,
            parent: current(),
        }
    }
}

/// Execute an object file with arguments.
///
/// ## Return
/// - `-1`: On error.
/// - `tid`: Tid of the newly spawned thread.
#[allow(unused_variables)]
pub fn execute(mut file: File, argv: Vec<String>) -> isize {
    #[cfg(feature = "debug")]
    kprintln!(
        "[PROCESS] Kernel thread {} prepare to execute a process with args {:?}",
        thread::current().name(),
        argv
    );

    // It only copies L2 pagetable. This approach allows the new thread
    // to access kernel code and data during syscall without the need to
    // swithch pagetables.
    let mut pt = KernelPgTable::clone();
    let id = Thread::get_and_increase_id();

    let (exec_info, stack_va) = match load::load_executable(&mut file, &mut pt, id) {
        Ok(x) => x,
        Err(_) => unsafe {
            pt.destroy();
            return -1;
        },
    };

    // Initialize frame, pass argument to user.
    let mut frame = unsafe { MaybeUninit::<Frame>::zeroed().assume_init() };
    frame.sepc = exec_info.entry_point;
    frame.x[2] = exec_info.init_sp;

    // Here the new process will be created.
    let userproc = UserProc::new(file);

    let mut argv_p: Vec<usize> = Vec::new();
    let mut current_p = stack_va as usize; // rsp

    unsafe fn fill<T>(pointer: usize, value: T) {
        *(pointer as *mut T) = value
    }

    unsafe {
        for argv in argv.iter().rev() {
            fill(current_p - 1, 0u8);
            current_p -= argv.len() + 1;
            copy_nonoverlapping(argv.as_ptr(), current_p as *mut u8, argv.len());
            argv_p.push(current_p);
        }

        current_p = align_ptr_floor(current_p - 1);
        current_p = align_ptr_floor(current_p - 1);
        // last pointer in argv is left zero
        fill(current_p, 0usize);

        for ptr in argv_p {
            current_p = align_ptr_floor(current_p - 1);
            fill(current_p, ptr - stack_va as usize + exec_info.init_sp)
        }
    }

    frame.x[2] += current_p - stack_va as usize;
    frame.x[10] = argv.len();
    frame.x[11] = frame.x[2];

    let real_id = thread::Builder::new(move || start(frame))
        .pagetable(pt)
        .userproc(userproc)
        .id(id)
        .spawn()
        .id();
    assert!(real_id == id);
    real_id
}

/// Exits a process.
///
/// Panic if the current thread doesn't own a user process.
pub fn exit(_value: isize) -> ! {
    let current = current();
    // kprintln!("thread {} exited with value {}", current.id(), _value);
    if let Some(userproc) = current.userproc.as_ref() {
        userproc.bin.to_owned().allow_write();
        let parent = userproc.parent.as_ref();
        parent
            .children
            .lock()
            .entry(current.id())
            .and_modify(|x| *x = ChildStatus::Exited(_value));
    } else {
        // panic!("current exiting thread doesn't own a user process");
    }
    thread::exit();
}

/// Waits for a child thread, which must own a user process.
///
/// ## Return
/// - `Some(exit_value)`
/// - `None`: if tid was not created by the current thread.
pub fn wait(_tid: isize) -> Option<isize> {
    let current = current();

    loop {
        let status = current.children.lock().get(&_tid)?.clone();
        match status {
            ChildStatus::Alive => schedule(),
            ChildStatus::Exited(status) => {
                current.children.lock().remove(&_tid);
                return Some(status);
            }
        }
    }
}

pub fn align_ptr_floor(ptr: usize) -> usize {
    return (ptr >> 3) << 3;
}

/// Initializes a user process in current thread.
///
/// This function won't return.
pub fn start(mut frame: Frame) -> ! {
    unsafe { sstatus::set_spp(sstatus::SPP::User) };
    frame.sstatus = sstatus::read();

    // Set kernel stack pointer to intr frame and then jump to `trap_exit_u()`.
    let kernal_sp = (&frame as *const Frame) as usize;

    unsafe {
        asm!(
            "mv sp, t0",
            "jr t1",
            in("t0") kernal_sp,
            in("t1") trap_exit_u as *const u8
        );
    }

    unreachable!();
}
