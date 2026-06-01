use crate::single_instance::SingleInstanceGuard;
use crate::win32::message_loop::MessageLoop;
use crate::audio::{self, LoopbackCapture, LoopbackCaptureConfig};

pub fn run() -> anyhow::Result<()> {
    let _instance = SingleInstanceGuard::acquire("Local\\TaskbarAudio_SingleInstance")?;
    let loopback = LoopbackCapture::start_default_loopback(LoopbackCaptureConfig::default())?;
    let _ = audio::set_global_stream(loopback.stream());
    let _loopback = loopback;
    let loop_ = MessageLoop::new()?;
    tracing::info!("app started");
    loop_.run()?;
    tracing::info!("app exited");
    Ok(())
}
