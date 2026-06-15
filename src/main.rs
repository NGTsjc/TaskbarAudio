#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod audio;
mod config;
mod error;
mod logging;
mod single_instance;
mod util;
mod win32;

use crate::single_instance::SingleInstanceGuard;
use crate::win32::message_loop::MessageLoop;
use crate::audio::{LoopbackCapture, LoopbackCaptureConfig};
use crate::util::mmsystem::HighResTimerGuard;

fn main() {
    if let Err(err) = run() {
        tracing::error!(error = %err, "fatal error");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let _timer_guard = HighResTimerGuard::new(1);
    let _logging_guard = logging::init()?;
    logging::install_panic_hook();
    config::user::load_user_config()?;
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