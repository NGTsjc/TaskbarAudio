mod loopback;
mod queue;
mod spectrum;
mod types;

use std::sync::RwLock;
pub use loopback::{AudioStream, LoopbackCapture, LoopbackCaptureConfig};
pub use spectrum::{SpectrumAnalyzer, SpectrumConfig, SpectrumFrame};

static GLOBAL_STREAM: RwLock<Option<AudioStream>> = RwLock::new(None);

pub fn set_global_stream(stream: Option<AudioStream>) {
    *GLOBAL_STREAM.write().unwrap_or_else(|e| e.into_inner()) = stream;
}

pub fn global_stream() -> Option<AudioStream> {
    GLOBAL_STREAM.read().unwrap_or_else(|e| e.into_inner()).clone()
}

impl AudioStream {
    pub fn pop_latest_spectrum(&self, analyzer: &mut SpectrumAnalyzer) -> Option<SpectrumFrame> {
        let block = self.pop_latest()?;
        analyzer.process_block(&block)
    }
}
