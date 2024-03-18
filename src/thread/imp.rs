//! Implementation of kernel threads

use alloc::boxed::Box;
use alloc::collections::BinaryHeap;
use alloc::sync::Arc;
use core::arch::global_asm;

use core::fmt::{self, Debug};
use core::sync::atomic::{AtomicIsize, AtomicU32, Ordering::SeqCst};

use crate::mem::{kalloc, kfree, PageTable, PG_SIZE};
use crate::sbi::interrupt;
use crate::thread::{current, schedule, Manager};
use crate::userproc::UserProc;

pub const PRI_DEFAULT: u32 = 31;
pub const PRI_MAX: u32 = 63;
pub const PRI_MIN: u32 = 0;
pub const STACK_SIZE: usize = PG_SIZE * 4;
pub const STACK_ALIGN: usize = 16;

pub type Mutex<T> = crate::sync::Mutex<T, crate::sync::Intr>;

// eraseable binary heap
#[derive(Default)]
pub struct EBinaryHeap(BinaryHeap<u32>, BinaryHeap<u32>);

impl EBinaryHeap {
    pub fn push(&mut self, key: u32) {
        self.0.push(key)
    }

    pub fn peek(&mut self) -> Option<&u32> {
        while let Some(peek) = self.1.peek() {
            if self.0.peek().unwrap() == peek {
                self.0.pop();
                self.1.pop();
            } else {
                break;
            }
        }
        self.0.peek()
    }

    pub fn erase(&mut self, key: u32) {
        self.1.push(key)
    }
}

/* --------------------------------- Thread --------------------------------- */
/// All data of a kernel thread
#[repr(C)]
pub struct Thread {
    tid: isize,
    name: &'static str,
    stack: usize,
    status: Mutex<Status>,
    context: Mutex<Context>,
    priority: AtomicU32,
    pub userproc: Option<UserProc>,
    pub pagetable: Option<Mutex<PageTable>>,
    // lock holder
    // Add Mutex<> only to make rust happy
    pub dependency: Mutex<Option<Arc<Thread>>>,
    // warning: to modify this variable, close interrupt first
    // because we should prevent other threads from updating their pirority
    // when we are dealing with the current modification.
    // Mutex<> on this is dangerous. SB compiler continuously complains about
    // the mutable borrow of EBinaryHeap, while in fact interrupts cannot
    // happen during the modification. Add Mutex<> only to make rust happy.
    pub donated_priorities: Mutex<EBinaryHeap>,
}

impl Thread {
    pub fn new(
        name: &'static str,
        stack: usize,
        priority: u32,
        entry: usize,
        userproc: Option<UserProc>,
        pagetable: Option<PageTable>,
    ) -> Self {
        /// The next thread's id
        static TID: AtomicIsize = AtomicIsize::new(0);

        Thread {
            tid: TID.fetch_add(1, SeqCst),
            name,
            stack,
            status: Mutex::new(Status::Ready),
            context: Mutex::new(Context::new(stack, entry)),
            priority: AtomicU32::new(priority),
            userproc,
            pagetable: pagetable.map(Mutex::new),
            dependency: Mutex::new(None),
            donated_priorities: Mutex::new(EBinaryHeap::default()),
        }
    }

    pub fn id(&self) -> isize {
        self.tid
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn status(&self) -> Status {
        *self.status.lock()
    }

    pub fn set_status(&self, status: Status) {
        *self.status.lock() = status;
    }

    pub fn context(&self) -> *mut Context {
        (&*self.context.lock()) as *const _ as *mut _
    }

    pub fn set_priority(&self, p: u32) {
        self.priority.store(p, SeqCst);
    }

    // turn interrupt off before entering this function
    // returns the effective priority of this thread
    pub fn priority(&self) -> u32 {
        let current = self.priority.load(SeqCst);
        match self.donated_priorities.lock().peek() {
            Some(p) => core::cmp::max(current, *p),
            None => current,
        }
    }

    // update donated priority in the dependency chain
    fn upload_prioriy(&self, previous: u32, modified: u32) {
        if previous != modified {
            let tmp = self.dependency.lock().clone();
            if let Some(dependency) = tmp {
                dependency.replace_donator(previous, modified);
            }
        }
    }

    // replace this thread's donated priority and upload
    fn replace_donator(&self, previous: u32, modified: u32) {
        let p = self.priority();

        let mut locked = self.donated_priorities.lock();
        locked.erase(previous);
        locked.push(modified);
        drop(locked);

        let m = self.priority();

        self.upload_prioriy(p, m);
    }

    pub fn add_donator(&self, priority: u32) {
        let previous = self.priority();
        // kprintln!("add: {}", priority);
        self.donated_priorities.lock().push(priority);
        let modified = self.priority();

        self.upload_prioriy(previous, modified);
    }

    pub fn remove_donator(&self, priority: u32) {
        assert!(self.dependency.lock().is_none());
        // kprintln!("remove: {}", priority);
        self.donated_priorities.lock().erase(priority);
    }
}

impl Debug for Thread {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_fmt(format_args!(
            "{}({})[{:?}]",
            self.name(),
            self.id(),
            self.status(),
        ))
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        #[cfg(feature = "debug")]
        kprintln!("[THREAD] {:?}'s resources are released", self);

        kfree(self.stack as *mut _, STACK_SIZE, STACK_ALIGN);
        if let Some(pt) = &self.pagetable {
            unsafe { pt.lock().destroy() };
        }
    }
}

/* --------------------------------- BUILDER -------------------------------- */
pub struct Builder {
    priority: u32,
    name: &'static str,
    function: usize,
    userproc: Option<UserProc>,
    pagetable: Option<PageTable>,
}

impl Builder {
    pub fn new<F>(function: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        // `*mut dyn FnOnce()` is a fat pointer, box it again to ensure FFI-safety.
        let function: *mut Box<dyn FnOnce()> = Box::into_raw(Box::new(Box::new(function)));

        Self {
            priority: PRI_DEFAULT,
            name: "Default",
            function: function as usize,
            userproc: None,
            pagetable: None,
        }
    }

    pub fn priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    pub fn name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }

    pub fn pagetable(mut self, pagetable: PageTable) -> Self {
        self.pagetable = Some(pagetable);
        self
    }

    pub fn userproc(mut self, userproc: UserProc) -> Self {
        self.userproc = Some(userproc);
        self
    }

    pub fn build(self) -> Arc<Thread> {
        let stack = kalloc(STACK_SIZE, STACK_ALIGN) as usize;

        Arc::new(Thread::new(
            self.name,
            stack,
            self.priority,
            self.function,
            self.userproc,
            self.pagetable,
        ))
    }

    /// Spawns a kernel thread and registers it to the [`Manager`].
    /// If it will run in the user environment, then two fields
    /// `userproc` and `pagetable` have to be set properly.
    ///
    /// Note that this function CANNOT be called during [`Manager`]'s initialization.
    pub fn spawn(self) -> Arc<Thread> {
        let new_thread = self.build();

        #[cfg(feature = "debug")]
        kprintln!("[THREAD] create {:?}", new_thread);

        Manager::get().register(new_thread.clone());

        // kprintln!(
        //     "spawning new thread with priority {}",
        //     new_thread.priority.load(SeqCst)
        // );

        // kprintln!("current priority is {}", current().priority.load(SeqCst));

        if new_thread.priority.load(SeqCst) > current().priority.load(SeqCst) {
            schedule()
        }

        // Off you go
        new_thread
    }
}

/* --------------------------------- Status --------------------------------- */
/// States of a thread's life cycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Not running but ready to run
    Ready,
    /// Currently running
    Running,
    /// Waiting for an event to trigger
    Blocked,
    /// About to be destroyed
    Dying,
}

/* --------------------------------- Context -------------------------------- */
/// Records a thread's running status when it switches to another thread,
/// and when switching back, restore its status from the context.
#[repr(C)]
#[derive(Debug)]
pub struct Context {
    /// return address
    ra: usize,
    /// kernel stack
    sp: usize,
    /// callee-saved
    s: [usize; 12],
}

impl Context {
    fn new(stack: usize, entry: usize) -> Self {
        Self {
            ra: kernel_thread_entry as usize,
            // calculate the address of stack top
            sp: stack + STACK_SIZE,
            // s0 stores a thread's entry point. For a new thread,
            // s0 will then be used as the first argument of `kernel_thread`.
            s: core::array::from_fn(|i| if i == 0 { entry } else { 0 }),
        }
    }
}

/* --------------------------- Kernel Thread Entry -------------------------- */

extern "C" {
    /// Entrance of kernel threads, providing a consistent entry point for all
    /// kernel threads (except the initial one). A thread gets here from `schedule_tail`
    /// when it's scheduled for the first time. To understand why program reaches this
    /// location, please check on the initial context setting in [`Manager::create`].
    ///
    /// A thread's actual entry is in `s0`, which is moved into `a0` here, and then
    /// it will be invoked in [`kernel_thread`].
    fn kernel_thread_entry() -> !;
}

global_asm! {r#"
    .section .text
        .globl kernel_thread_entry
    kernel_thread_entry:
        mv a0, s0
        j kernel_thread
"#}

/// Executes the `main` function of a kernel thread. Once a thread is finished, mark
/// it as [`Dying`](Status::Dying).
#[no_mangle]
extern "C" fn kernel_thread(main: *mut Box<dyn FnOnce()>) -> ! {
    let main = unsafe { Box::from_raw(main) };

    interrupt::set(true);

    main();

    super::exit()
}
