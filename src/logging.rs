use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;

pub struct LoggingGuard {
    _file: Option<Arc<Mutex<File>>>,
}

pub fn init() -> anyhow::Result<LoggingGuard> {
    let log_level = std::env::var("TASKBARAUDIO_LOG_LEVEL").unwrap_or_else(|_| "debug".to_string());
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(&log_level)
    });

    let file_enabled = std::env::var("TASKBARAUDIO_LOG_FILE").is_ok();
    if file_enabled && let Ok(guard) = init_file_logger(env_filter.clone()) {
        return Ok(guard);
    }

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(LoggingGuard { _file: None })
}

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "unknown panic payload"
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".to_string());
        tracing::error!(panic = %payload, location = %location, "panic occurred");
    }));
}

#[derive(Clone)]
struct FileMakeWriter {
    file: Arc<Mutex<File>>,
}

impl<'a> MakeWriter<'a> for FileMakeWriter {
    type Writer = FileWriter;
    fn make_writer(&'a self) -> Self::Writer {
        FileWriter { file: Arc::clone(&self.file) }
    }
}

struct FileWriter {
    file: Arc<Mutex<File>>,
}

impl Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = self.file.lock().unwrap();
        file.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self.file.lock().unwrap();
        file.flush()
    }
}

fn default_log_dir() -> PathBuf {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(local_app_data).join("TaskbarAudio").join("logs");
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("logs")
}

fn init_file_logger(env_filter: EnvFilter) -> anyhow::Result<LoggingGuard> {
    let log_dir = default_log_dir();
    std::fs::create_dir_all(&log_dir)?;
    
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let log_file_path = log_dir.join(format!("taskbar_audio_{}.log", timestamp));
    
    let mut log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)?;
    
    writeln!(&mut log_file)?;
    let file = Arc::new(Mutex::new(log_file));
    let make_writer = FileMakeWriter { file: Arc::clone(&file) };
    
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(make_writer)
        .with_ansi(false)
        .try_init()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    
    Ok(LoggingGuard { _file: Some(file) })
}

pub mod macros {
    #[macro_export]
    macro_rules! trace {
        ($($arg:tt)*) => {
            tracing::trace!($($arg)*)
        };
    }

    #[macro_export]
    macro_rules! debug {
        ($($arg:tt)*) => {
            tracing::debug!($($arg)*)
        };
    }

    #[macro_export]
    macro_rules! info {
        ($($arg:tt)*) => {
            tracing::info!($($arg)*)
        };
    }

    #[macro_export]
    macro_rules! warn {
        ($($arg:tt)*) => {
            tracing::warn!($($arg)*)
        };
    }

    #[macro_export]
    macro_rules! error {
        ($($arg:tt)*) => {
            tracing::error!($($arg)*)
        };
    }
}
