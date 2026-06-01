use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use crate::audio::types::AudioBlock;

#[derive(Clone)]
pub struct AudioQueue {
    inner: Arc<Mutex<VecDeque<AudioBlock>>>,
    capacity: usize,
}

impl AudioQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    pub fn push(&self, block: AudioBlock) {
        let mut q = self.inner.lock().unwrap();
        while q.len() >= self.capacity {
            q.pop_front();
        }
        q.push_back(block);
    }

    pub fn pop_latest(&self) -> Option<AudioBlock> {
        let mut q = self.inner.lock().unwrap();
        if q.is_empty() {
            return None;
        }
        let latest = q.pop_back();
        q.clear();
        latest
    }

    pub fn len(&self) -> usize {
        let q = self.inner.lock().unwrap();
        q.len()
    }
}
