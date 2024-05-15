use alloc::{collections::VecDeque, sync::Arc};

use crate::thread::{Schedule, Thread};

#[derive(Default)]
pub struct Fcfs(VecDeque<Arc<Thread>>);

impl Schedule for Fcfs {
    fn register(&mut self, thread: Arc<Thread>) {
        self.0.push_front(thread)
    }

    fn schedule(&mut self) -> Option<Arc<Thread>> {
        self.0.pop_back()
    }

    fn next(&mut self) -> Option<Arc<Thread>> {
        self.0.back().map(|x| x.clone())
    }
}
