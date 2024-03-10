use core::sync::atomic::Ordering::SeqCst;

use alloc::sync::Arc;

use crate::{
    pq::FIFOPrioriyQueue,
    thread::{self, Schedule},
};

#[derive(Clone)]
pub struct Thread(pub Arc<thread::Thread>);

impl PartialEq for Thread {
    fn eq(&self, other: &Self) -> bool {
        let p_this = self.0.priority.load(SeqCst);
        let p_other = other.0.priority.load(SeqCst);
        p_this == p_other
    }
}

impl PartialOrd for Thread {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        let p_this = self.0.priority.load(SeqCst);
        let p_other = other.0.priority.load(SeqCst);
        Some(p_this.cmp(&p_other))
    }
}

impl Eq for Thread {}

impl Ord for Thread {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl From<Arc<thread::Thread>> for Thread {
    fn from(value: Arc<thread::Thread>) -> Self {
        Thread(value)
    }
}
/// FIFO scheduler.
#[derive(Default)]
pub struct Fcfs(FIFOPrioriyQueue<Thread>);

impl Schedule for Fcfs {
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
