use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "taskbaraudio-locale-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.join("locale.json"));
        let _ = std::fs::remove_dir(&self.0);
    }
}

#[test]
fn partial_overrides_preserve_english_defaults() {
    let text =
        parse(r#"{"show_visualizer":"  Montrer  ","exit":"Quitter","future_key":"ignored"}"#)
            .unwrap();
    assert_eq!(text.show_visualizer, "Montrer");
    assert_eq!(text.exit, "Quitter");
    assert_eq!(text.hide_visualizer, "Hide Visualizer");
    assert_eq!(text.retry_taskbar_attachment, "Retry Taskbar Attachment");
    assert_eq!(text.restart, "Restart");
    assert_eq!(parse("{}").unwrap(), UiText::default());
}

#[test]
fn invalid_fields_fall_back_individually() {
    for invalid in [
        serde_json::json!(null),
        serde_json::json!(12),
        serde_json::json!(false),
        serde_json::json!([]),
        serde_json::json!({}),
        serde_json::json!(""),
        serde_json::json!("  "),
        serde_json::json!("Exit\0hidden"),
        serde_json::json!("Exit\tCtrl+Q"),
        serde_json::json!("Exit\nnow"),
    ] {
        let mut object = serde_json::Map::new();
        for key in [
            "show_visualizer",
            "hide_visualizer",
            "retry_taskbar_attachment",
            "restart",
        ] {
            object.insert(key.into(), invalid.clone());
        }
        object.insert("exit".into(), serde_json::json!("Quitter"));
        let text = parse(&serde_json::Value::Object(object).to_string()).unwrap();
        assert_eq!(
            text,
            UiText {
                exit: "Quitter".into(),
                ..UiText::default()
            }
        );
    }
    assert_eq!(parse(r#"{"exit":123}"#).unwrap().exit, "Exit");
}

#[test]
fn bom_unicode_and_shipped_example_are_supported() {
    let text =
        parse("\u{feff}{\"show_visualizer\":\"\u{663e}\u{793a}\",\"exit\":\"Salir\"}").unwrap();
    assert_eq!(text.show_visualizer, "\u{663e}\u{793a}");
    assert_eq!(text.exit, "Salir");
    let example = parse(include_str!("../asset/locale.example.json")).unwrap();
    assert_ne!(example, UiText::default());
    assert_eq!(example.exit, "\u{9000}\u{51fa}");
    for invalid in ["{", "[]", "null", "12", "true", "\"text\""] {
        assert!(parse(invalid).is_err());
    }
}

#[test]
fn optional_files_resolve_against_exe_directory_and_fail_safely() {
    let dir = TestDir::new();
    for filename in [None, Some(""), Some("  "), Some("missing.json"), Some(".")] {
        assert_eq!(load_from_dir(&dir.0, filename), UiText::default());
    }
    let path = dir.0.join("locale.json");
    std::fs::write(&path, "\u{feff}{\"restart\":\"Reiniciar\"}").unwrap();
    for filename in ["locale.json", path.to_str().unwrap()] {
        assert_eq!(load_from_dir(&dir.0, Some(filename)).restart, "Reiniciar");
    }
    assert_eq!(load_from_dir(&dir.0, None), UiText::default());
    assert_eq!(load_from_dir(&dir.0, Some(" ")), UiText::default());
    for bytes in [b"{".as_slice(), b"[]".as_slice(), &[0xff, 0xfe]] {
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(
            load_from_dir(&dir.0, Some("locale.json")),
            UiText::default()
        );
    }
}
