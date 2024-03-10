use alloc::collections::BinaryHeap;

#[derive(Default, Clone)]
struct PrioritizedItem<T> {
    item: T,
    insertion_order: usize,
}

impl<T: Eq> Eq for PrioritizedItem<T> {}

impl<T: Eq> PartialEq for PrioritizedItem<T> {
    fn eq(&self, other: &Self) -> bool {
        self.item == other.item && self.insertion_order == other.insertion_order
    }
}

impl<T: Ord> Ord for PrioritizedItem<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.item.cmp(&other.item) {
            core::cmp::Ordering::Equal => other.insertion_order.cmp(&self.insertion_order),
            ord => ord,
        }
    }
}

impl<T: Ord> PartialOrd for PrioritizedItem<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        match self.item.partial_cmp(&other.item) {
            Some(core::cmp::Ordering::Equal) => {
                other.insertion_order.partial_cmp(&self.insertion_order)
            }
            ord => ord,
        }
    }
}

#[derive(Clone)]
pub struct FIFOPrioriyQueue<T> {
    items: BinaryHeap<PrioritizedItem<T>>,
    insertion_order: usize,
}

impl<T: Ord> FIFOPrioriyQueue<T> {
    pub fn new() -> Self {
        Self {
            items: BinaryHeap::new(),
            insertion_order: 0,
        }
    }

    pub fn push(&mut self, item: T) {
        self.insertion_order += 1;
        self.items.push(PrioritizedItem {
            item,
            insertion_order: self.insertion_order,
        })
    }

    pub fn pop(&mut self) -> Option<T> {
        self.items.pop().map(|item| item.item)
    }

    pub fn peek(&self) -> Option<&T> {
        self.items.peek().map(|item| &item.item)
    }
}

impl<T: Ord> Default for FIFOPrioriyQueue<T> {
    fn default() -> Self {
        Self {
            items: Default::default(),
            insertion_order: Default::default(),
        }
    }
}
