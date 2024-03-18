use alloc::collections::VecDeque;

#[derive(Clone)]
pub struct FIFOPrioriyQueue<T> {
    // BinaryHeap will be garbled due to the priority donation
    // I don't know how to efficiently tackle this
    items: VecDeque<T>,
}

impl<T: Ord> FIFOPrioriyQueue<T> {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
        }
    }

    pub fn push(&mut self, item: T) {
        self.items.push_front(item);
    }

    pub fn pop(&mut self) -> Option<T> {
        if let Some((index, _)) = self.items.iter().enumerate().max_by_key(|(_, x)| *x) {
            return self.items.remove(index);
        }
        None
    }

    pub fn peek(&self) -> Option<&T> {
        self.items
            .iter()
            .enumerate()
            .max_by_key(|(_, x)| *x)
            .map(|x| x.1)
    }
}

impl<T: Ord> Default for FIFOPrioriyQueue<T> {
    fn default() -> Self {
        Self {
            items: Default::default(),
        }
    }
}
