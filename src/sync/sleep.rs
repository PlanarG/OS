use alloc::sync::Arc;
#[cfg(feature = "thread-scheduler-priority")]
use alloc::vec::Vec;
use core::cell::RefCell;

#[cfg(feature = "thread-scheduler-priority")]
use crate::sbi;
use crate::sync::{Lock, Semaphore};
use crate::thread::{self, Thread};

/// Sleep lock. Uses [`Semaphore`] under the hood.
#[derive(Clone)]
pub struct Sleep {
    inner: Semaphore,
    holder: RefCell<Option<Arc<Thread>>>,
    #[cfg(feature = "thread-scheduler-priority")]
    waiter: RefCell<Vec<Arc<Thread>>>,
}

impl Default for Sleep {
    fn default() -> Self {
        Self {
            inner: Semaphore::new(1),
            holder: Default::default(),
            #[cfg(feature = "thread-scheduler-priority")]
            waiter: Default::default(),
        }
    }
}

#[cfg(feature = "thread-scheduler-priority")]
impl Lock for Sleep {
    fn acquire(&self) {
        let old = sbi::interrupt::set(false);
        let current = thread::current();

        // kprintln!("thread {} aquires the lock", current.id());

        assert!(current.dependency.lock().is_none());

        self.waiter.borrow_mut().push(current.clone());
        if let Some(holder) = self.holder.borrow().as_ref() {
            // kprintln!("... which is held by {}", holder.id());
            holder.add_donator(current.priority());
            current.dependency.lock().replace(holder.clone());
        }

        self.inner.down();

        let index = self
            .waiter
            .borrow_mut()
            .iter()
            .position(|x| x.id() == thread::current().id())
            .unwrap();
        self.waiter.borrow_mut().remove(index);

        self.waiter.borrow_mut().iter().for_each(|x| {
            current.add_donator(x.priority());
            x.dependency.lock().replace(current.clone());
        });

        assert!(current.dependency.lock().is_none());

        self.holder.borrow_mut().replace(current);

        sbi::interrupt::set(old);
    }

    fn release(&self) {
        let old = sbi::interrupt::set(false);

        let current = thread::current();

        // kprintln!("thread {} releases the lock. ", current.id());

        assert!(Arc::ptr_eq(
            self.holder.borrow().as_ref().unwrap(),
            &current
        ));

        // kprint!("waiter list: ");
        self.waiter.borrow_mut().iter().for_each(|x| {
            // kprint!("{}, ", x.id());
            current.remove_donator(x.priority());
            x.dependency.lock().take().unwrap();
        });
        // kprint!("\n");

        assert!(current.dependency.lock().is_none());

        self.holder.borrow_mut().take().unwrap();
        self.inner.up();

        sbi::interrupt::set(old);
    }
}

#[cfg(not(feature = "thread-scheduler-priority"))]
impl Lock for Sleep {
    fn acquire(&self) {
        self.inner.down();
        self.holder.borrow_mut().replace(thread::current());
    }

    fn release(&self) {
        assert!(Arc::ptr_eq(
            self.holder.borrow().as_ref().unwrap(),
            &thread::current()
        ));

        self.holder.borrow_mut().take().unwrap();
        self.inner.up();
    }
}

unsafe impl Sync for Sleep {}
