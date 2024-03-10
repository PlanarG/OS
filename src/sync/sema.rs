use core::cell::{Cell, RefCell};

use crate::pq::FIFOPrioriyQueue;
use crate::sbi;
use crate::thread::scheduler::fcfs::Thread;
use crate::thread::{self};

/// Atomic counting semaphore
///
/// # Examples
/// ```
/// let sema = Semaphore::new(0);
/// sema.down();
/// sema.up();
/// ```
#[derive(Clone)]
pub struct Semaphore {
    value: Cell<usize>,
    waiters: RefCell<FIFOPrioriyQueue<Thread>>,
}

unsafe impl Sync for Semaphore {}
unsafe impl Send for Semaphore {}

impl Semaphore {
    /// Creates a new semaphore of initial value n.
    pub fn new(n: usize) -> Self {
        Semaphore {
            value: Cell::new(n),
            waiters: RefCell::new(FIFOPrioriyQueue::new()),
        }
    }

    /// P operation
    pub fn down(&self) {
        let old = sbi::interrupt::set(false);

        // Is semaphore available?
        while self.value() == 0 {
            // `push_front` ensures to wake up threads in a fifo manner
            self.waiters.borrow_mut().push(thread::current().into());

            // Block the current thread until it's awakened by an `up` operation
            thread::block();
        }
        self.value.set(self.value() - 1);

        sbi::interrupt::set(old);
    }

    /// V operation
    pub fn up(&self) {
        let old = sbi::interrupt::set(false);
        self.value.replace(self.value() + 1);
        let result = self.waiters.borrow_mut().pop();

        // Check if we need to wake up a sleeping waiter
        if let Some(thread) = result {
            // assert_eq!(count, 0);
            thread::wake_up(thread.clone().0);
        }

        sbi::interrupt::set(old);
    }

    /// Get the current value of a semaphore
    pub fn value(&self) -> usize {
        self.value.get()
    }
}
