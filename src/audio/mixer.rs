use std::collections::VecDeque;
use std::time::Duration;

// Both inputs have the same format (WASAPI converts the microphone on capture).
// Keep short bounded FIFOs so a silent endpoint or clock drift cannot build lag.
pub(super) struct Mixer {
    inputs: [VecDeque<f32>; 2],
    block_samples: usize,
    block_duration: Duration,
    last_output: Duration,
    missing_input: Option<usize>,
}

impl Mixer {
    pub fn new(frames: usize, channels: u16, sample_rate: u32) -> Self {
        Self {
            inputs: Default::default(),
            block_samples: frames * channels as usize,
            block_duration: Duration::from_secs_f64(frames as f64 / sample_rate as f64),
            last_output: Duration::ZERO,
            missing_input: None,
        }
    }

    pub fn push(&mut self, input: usize, samples: Vec<f32>) {
        let queue = &mut self.inputs[input];
        queue.extend(samples);
        let limit = self.block_samples * 4;
        if queue.len() > limit {
            // Drop whole frames: all packet and block sizes are channel-aligned.
            queue.drain(..queue.len() - limit);
        }
    }

    pub fn pop(&mut self, now: Duration) -> Option<Vec<f32>> {
        let both_ready = self.inputs.iter().all(|q| q.len() >= self.block_samples);
        let elapsed = now.saturating_sub(self.last_output);
        let any_samples = self.inputs.iter().any(|q| !q.is_empty());
        let missing = self
            .inputs
            .iter()
            .position(|q| q.len() < self.block_samples);
        // Give the other endpoint two block periods to supply a delayed packet.
        // Once it is known to be silent, consume the active input at its full
        // sample rate instead of repeatedly waiting two periods and dropping half.
        let wait = if any_samples && missing != self.missing_input {
            self.block_duration * 2
        } else {
            self.block_duration
        };
        if !both_ready && elapsed < wait {
            return None;
        }
        self.missing_input = missing;
        let mut samples = Vec::with_capacity(self.block_samples);
        for _ in 0..self.block_samples {
            let a = self.inputs[0].pop_front().unwrap_or(0.0);
            let b = self.inputs[1].pop_front().unwrap_or(0.0);
            samples.push((a + b) * 0.5);
        }
        self.last_output = now;
        Some(samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_packet_boundaries_before_mixing() {
        let mut mixer = Mixer::new(4, 1, 1000);
        mixer.push(0, vec![1.0; 4]);
        mixer.push(1, vec![0.5; 2]);
        assert!(mixer.pop(Duration::from_millis(1)).is_none());
        mixer.push(1, vec![0.5; 2]);
        assert_eq!(mixer.pop(Duration::from_millis(2)).unwrap(), vec![0.75; 4]);
    }

    #[test]
    fn missing_input_and_silence_do_not_flood_queue() {
        let mut mixer = Mixer::new(480, 1, 48_000);
        mixer.push(0, vec![1.0; 480]);
        assert!(mixer.pop(Duration::from_millis(19)).is_none());
        assert_eq!(
            mixer.pop(Duration::from_millis(20)).unwrap(),
            vec![0.5; 480]
        );
        for ms in 21..30 {
            assert!(mixer.pop(Duration::from_millis(ms)).is_none());
        }
        assert_eq!(
            mixer.pop(Duration::from_millis(30)).unwrap(),
            vec![0.0; 480]
        );
    }

    #[test]
    fn lagging_input_keeps_bounded_recent_audio() {
        let mut mixer = Mixer::new(4, 2, 1000);
        for value in 0..100 {
            mixer.push(0, vec![value as f32; 8]);
        }
        assert_eq!(mixer.inputs[0].len(), 32);
        mixer.push(1, vec![0.0; 8]);
        assert_eq!(mixer.pop(Duration::ZERO).unwrap(), vec![48.0; 8]);
    }

    #[test]
    fn one_silent_endpoint_keeps_the_other_at_full_rate() {
        let mut mixer = Mixer::new(480, 1, 48_000);
        mixer.push(1, vec![0.5; 480]);
        assert_eq!(
            mixer.pop(Duration::from_millis(20)).unwrap(),
            vec![0.25; 480]
        );
        for now in (30..=1000).step_by(10) {
            mixer.push(1, vec![0.5; 480]);
            assert_eq!(
                mixer.pop(Duration::from_millis(now)).unwrap(),
                vec![0.25; 480]
            );
            assert!(mixer.inputs[1].is_empty());
        }
    }
}
