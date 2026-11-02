use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, PartialEq, Eq)]
pub struct UiText {
    pub show_visualizer: String,
    pub hide_visualizer: String,
    pub retry_taskbar_attachment: String,
    pub restart: String,
    pub exit: String,
}

impl Default for UiText {
    fn default() -> Self {
        Self {
            show_visualizer: "Show Visualizer".into(),
            hide_visualizer: "Hide Visualizer".into(),
            retry_taskbar_attachment: "Retry Taskbar Attachment".into(),
            restart: "Restart".into(),
            exit: "Exit".into(),
        }
    }
}

static UI_TEXT: OnceLock<UiText> = OnceLock::new();

pub fn init(language_file: Option<&str>) {
    UI_TEXT.get_or_init(|| {
        let Some(file) = language_file.map(str::trim).filter(|file| !file.is_empty()) else {
            return UiText::default();
        };
        match std::env::current_exe() {
            Ok(exe) => match exe.parent() {
                Some(dir) => load_from_dir(dir, Some(file)),
                None => {
                    tracing::warn!("cannot locate executable directory; using English UI text");
                    UiText::default()
                }
            },
            Err(error) => {
                tracing::warn!(%error, "cannot locate executable; using English UI text");
                UiText::default()
            }
        }
    });
}

pub fn text() -> &'static UiText {
    // Native tests can create a tray without going through the app entry point.
    UI_TEXT.get_or_init(UiText::default)
}

fn load_from_dir(exe_dir: &Path, language_file: Option<&str>) -> UiText {
    let Some(file) = language_file.map(str::trim).filter(|file| !file.is_empty()) else {
        return UiText::default();
    };
    let path = exe_dir.join(file);
    let result = std::fs::read_to_string(&path)
        .map_err(anyhow::Error::from)
        .and_then(|content| parse(&content));
    match result {
        Ok(text) => {
            tracing::info!(path = %path.display(), "custom UI text loaded");
            text
        }
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "language file unavailable or invalid; using English UI text");
            UiText::default()
        }
    }
}

fn parse(content: &str) -> anyhow::Result<UiText> {
    let value: serde_json::Value = serde_json::from_str(content.trim_start_matches('\u{feff}'))?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("language file must contain a JSON object"))?;
    let mut text = UiText::default();
    for (key, target) in [
        ("show_visualizer", &mut text.show_visualizer),
        ("hide_visualizer", &mut text.hide_visualizer),
        (
            "retry_taskbar_attachment",
            &mut text.retry_taskbar_attachment,
        ),
        ("restart", &mut text.restart),
        ("exit", &mut text.exit),
    ] {
        let Some(value) = object.get(key) else {
            continue;
        };
        match value.as_str() {
            Some(label) if !label.trim().is_empty() && !label.chars().any(char::is_control) => {
                *target = label.trim().to_owned();
            }
            _ => tracing::warn!(
                key,
                "UI label must be a nonempty string without control characters; using English default"
            ),
        }
    }
    Ok(text)
}

#[cfg(test)]
#[path = "localization_tests.rs"]
mod tests;
