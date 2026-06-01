#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod audio;
mod config;
mod logging;
mod single_instance;
mod win32;

fn main() {
    if let Err(err) = run() {
        tracing::error!(error = %err, "fatal error");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let _logging_guard = logging::init()?;
    logging::install_panic_hook();
    app::run()
}
