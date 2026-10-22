#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod audio;
mod autostart;
mod config;
mod logging;
mod single_instance;
mod util;
mod win32;

use crate::single_instance::SingleInstanceGuard;
use crate::win32::message_loop::MessageLoop;

fn main() {
    if let Err(err) = run() {
        tracing::error!(error = %err, "fatal error");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let _logging_guard = logging::init()?;
    logging::install_panic_hook();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "app boot");
    config::user::load_user_config()?;
    let user_config = config::user::get_user_config();
    let should_restart = {
        let _instance = SingleInstanceGuard::acquire("Local\\TaskbarAudio_SingleInstance")?;
        if let Err(err) = autostart::ensure_auto_start(user_config.auto_start) {
            tracing::warn!(error = %err, enabled = user_config.auto_start, "sync auto start failed");
        }
        let loop_ = MessageLoop::new()?;
        tracing::info!(version = env!("CARGO_PKG_VERSION"), "app started");
        loop_.run()?;
        crate::win32::message_loop::take_restart_requested()
    };
    if should_restart {
        restart_current_process()?;
    }
    tracing::info!("app exited");
    Ok(())
}

fn restart_current_process() -> anyhow::Result<()> {
    let exe_path = std::env::current_exe()?;
    tracing::info!(path = %exe_path.display(), "restarting app");
    let mut command = std::process::Command::new(&exe_path);
    if let Some(exe_dir) = exe_path.parent() {
        command.current_dir(exe_dir);
    }
    command.spawn()?;
    Ok(())
}
