#[cfg(feature = "thread-scheduler-priority")]
use alloc::sync::Arc;

#[cfg(feature = "thread-scheduler-priority")]
use crate::{
    pq::FIFOPrioriyQueue,
    thread::{self, Schedule},
};

#[cfg(feature = "thread-scheduler-priority")]
#[derive(Clone)]
pub struct Thread(pub Arc<thread::Thread>);

#[cfg(feature = "thread-scheduler-priority")]
impl PartialEq for Thread {
    fn eq(&self, other: &Self) -> bool {
        let p_this = self.0.priority();
        let p_other = other.0.priority();
        p_this == p_other
    }
}

#[cfg(feature = "thread-scheduler-priority")]
impl PartialOrd for Thread {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        let p_this = self.0.priority();
        let p_other = other.0.priority();
        Some(p_this.cmp(&p_other))
    }
}

#[cfg(feature = "thread-scheduler-priority")]
impl Eq for Thread {}

#[cfg(feature = "thread-scheduler-priority")]
impl Ord for Thread {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[cfg(feature = "thread-scheduler-priority")]
impl From<Arc<thread::Thread>> for Thread {
    fn from(value: Arc<thread::Thread>) -> Self {
        Thread(value)
    }
}
/// FIFO scheduler.
#[cfg(feature = "thread-scheduler-priority")]
#[derive(Default)]
pub struct Priority(FIFOPrioriyQueue<Thread>);

#[cfg(feature = "thread-scheduler-priority")]
impl Schedule for Priority {
    fn register(&mut self, thread: Arc<thread::Thread>) {
        self.0.push(thread.into())
    }

    fn schedule(&mut self) -> Option<Arc<thread::Thread>> {
        self.0.pop().map(|thread| thread.0)
    }

    fn next(&mut self) -> Option<Arc<thread::Thread>> {
        self.0.peek().map(|thread| thread.0.clone())
    }
}
