use crossbeam_queue::ArrayQueue;
use std::sync::Arc;
use crate::audio::types::AudioBlock;

pub struct AudioQueue {
    inner: Arc<ArrayQueue<AudioBlock>>,
}

impl Clone for AudioQueue {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl AudioQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(ArrayQueue::new(capacity)),
        }
    }

    pub fn push(&self, block: AudioBlock) {
        while self.inner.push(block.clone()).is_err() {
            let _ = self.inner.pop();
        }
    }

    pub fn pop_latest(&self) -> Option<AudioBlock> {
        let mut last = None;
        while let Some(block) = self.inner.pop() {
            last = Some(block);
        }
        last
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}