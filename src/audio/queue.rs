use crate::audio::types::AudioBlock;
use crossbeam_queue::ArrayQueue;
use std::sync::Arc;

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
        // Move the samples into the queue; replace the oldest block if full.
        self.inner.force_push(block);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::types::AudioFormat;

    #[test]
    fn keeps_newest_block_without_copying_samples() {
        let queue = AudioQueue::new(2);
        let mut latest_ptr = std::ptr::null();
        for value in 0..8 {
            let samples = vec![value as f32; 512];
            latest_ptr = samples.as_ptr();
            queue.push(AudioBlock {
                format: AudioFormat {
                    sample_rate: 48_000,
                    channels: 1,
                },
                frames: samples.len(),
                samples,
            });
            assert!(queue.len() <= 2);
        }
        let block = queue.pop_latest().unwrap();
        assert_eq!(block.samples[0], 7.0);
        assert_eq!(block.samples.as_ptr(), latest_ptr);
        assert!(queue.pop_latest().is_none());
    }
}
