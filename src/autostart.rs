use std::path::Path;
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

const RUN_KEY_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE_NAME: &str = "TaskbarAudio";

pub fn ensure_auto_start(enabled: bool) -> anyhow::Result<()> {
    if enabled {
        enable_auto_start()
    } else {
        disable_auto_start()
    }
}

pub fn enable_auto_start() -> anyhow::Result<()> {
    let exe_path = std::env::current_exe()?;
    let value = build_run_value(&exe_path);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run_key, _) = hkcu.create_subkey(RUN_KEY_PATH)?;
    run_key.set_value(RUN_VALUE_NAME, &value)?;
    tracing::info!(path = %exe_path.display(), "auto start enabled");
    Ok(())
}

pub fn disable_auto_start() -> anyhow::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu.open_subkey_with_flags(RUN_KEY_PATH, winreg::enums::KEY_SET_VALUE);
    match run_key {
        Ok(key) => match key.delete_value(RUN_VALUE_NAME) {
            Ok(()) => tracing::info!("auto start disabled"),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!("auto start entry not found, skip delete");
            }
            Err(err) => return Err(err.into()),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!("run registry key not found, skip delete");
        }
        Err(err) => return Err(err.into()),
    }
    Ok(())
}

fn build_run_value(exe_path: &Path) -> String {
    format!("\"{}\"", exe_path.display())
}

#[cfg(test)]
mod tests {
    use super::build_run_value;
    use std::path::Path;

    #[test]
    fn run_value_is_quoted() {
        let value = build_run_value(Path::new(r"D:\My App\TaskbarAudio\taskbar_audio.exe"));
        assert_eq!(value, r#""D:\My App\TaskbarAudio\taskbar_audio.exe""#);
    }
}
