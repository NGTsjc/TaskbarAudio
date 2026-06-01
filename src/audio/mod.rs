mod loopback;
mod queue;
mod spectrum;
mod types;

use std::sync::OnceLock;
pub use loopback::{AudioStream, LoopbackCapture, LoopbackCaptureConfig};
pub use queue::AudioQueue;
pub use spectrum::{SpectrumAnalyzer, SpectrumConfig, SpectrumFrame, WindowFunction};
pub use types::{AudioBlock, AudioEnergy, AudioFormat};

static GLOBAL_STREAM: OnceLock<AudioStream> = OnceLock::new();

pub fn set_global_stream(stream: AudioStream) -> bool {
    GLOBAL_STREAM.set(stream).is_ok()
}

pub fn global_stream() -> Option<&'static AudioStream> {
    GLOBAL_STREAM.get()
}

impl AudioStream {
    pub fn pop_latest_spectrum(&self, analyzer: &mut SpectrumAnalyzer) -> Option<SpectrumFrame> {
        let block = self.pop_latest()?;
        analyzer.process_block(&block)
    }
}
