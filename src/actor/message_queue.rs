use std::{any::Any, collections::VecDeque};

// TODO: Rework this to use a intrusive linked list
// TODO: Introduce an 'Envelope' and 'Message' type
pub struct MessageQueue {
    queue: VecDeque<Box<dyn Any + Send>>,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn push(&mut self, envelope: Box<dyn Any + Send>) {
        self.queue.push_back(envelope);
    }

    pub fn remove_matching(
        &mut self,
        matcher: &dyn Fn(&Box<dyn Any + Send>) -> bool,
    ) -> Option<Box<dyn Any + Send>> {
        if let Some(index) = self.queue.iter().position(|msg| matcher(msg)) {
            self.queue.remove(index)
        } else {
            None
        }
    }
}
