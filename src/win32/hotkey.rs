use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};

pub const TOGGLE_HOTKEY_ID: i32 = 1;

#[derive(Debug, PartialEq, Eq)]
struct HotkeySpec {
    modifiers: HOT_KEY_MODIFIERS,
    key: u32,
}

impl HotkeySpec {
    fn parse(value: &str) -> anyhow::Result<Self> {
        let parts: Vec<_> = value.split('+').map(str::trim).collect();
        anyhow::ensure!(parts.len() >= 2, "a modifier and a main key are required");
        let mut modifiers = HOT_KEY_MODIFIERS(0);
        for part in &parts[..parts.len() - 1] {
            let modifier = match part.to_ascii_lowercase().as_str() {
                "ctrl" => MOD_CONTROL,
                "alt" => MOD_ALT,
                "shift" => MOD_SHIFT,
                "win" => MOD_WIN,
                _ => anyhow::bail!("unknown modifier: {part}"),
            };
            anyhow::ensure!(modifiers.0 & modifier.0 == 0, "duplicate modifier: {part}");
            modifiers |= modifier;
        }
        let name = parts.last().unwrap().to_ascii_uppercase();
        let key = match name.as_str() {
            "SPACE" => 0x20,
            "TAB" => 0x09,
            "ENTER" => 0x0D,
            "ESCAPE" => 0x1B,
            "BACKSPACE" => 0x08,
            "DELETE" => 0x2E,
            "INSERT" => 0x2D,
            "HOME" => 0x24,
            "END" => 0x23,
            "PAGEUP" => 0x21,
            "PAGEDOWN" => 0x22,
            "LEFT" => 0x25,
            "RIGHT" => 0x27,
            "UP" => 0x26,
            "DOWN" => 0x28,
            _ if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() => {
                name.as_bytes()[0] as u32
            }
            _ => {
                let number = name.strip_prefix('F').and_then(|v| v.parse::<u32>().ok());
                match number {
                    Some(n @ 1..=24) if n != 12 && name == format!("F{n}") => 0x70 + n - 1,
                    _ => anyhow::bail!("unsupported or reserved main key: {name}"),
                }
            }
        };
        Ok(Self { modifiers, key })
    }
}

pub fn register(hwnd: HWND, configured: Option<&str>) {
    let Some(value) = configured.map(str::trim).filter(|v| !v.is_empty()) else {
        return;
    };
    let spec = match HotkeySpec::parse(value) {
        Ok(spec) => spec,
        Err(error) => {
            tracing::warn!(hotkey = value, %error, "invalid toggle hotkey; ignoring");
            return;
        }
    };
    match unsafe {
        RegisterHotKey(
            hwnd,
            TOGGLE_HOTKEY_ID,
            spec.modifiers | MOD_NOREPEAT,
            spec.key,
        )
    } {
        Ok(()) => tracing::info!(hotkey = value, "toggle hotkey registered"),
        Err(error) => tracing::warn!(hotkey = value, %error, "toggle hotkey unavailable; ignoring"),
    }
}

pub fn unregister(hwnd: HWND) {
    // An absent/failed registration is harmless during cleanup.
    let _ = unsafe { UnregisterHotKey(hwnd, TOGGLE_HOTKEY_ID) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_combinations() {
        let spec = HotkeySpec::parse(" cTrL + aLt + v ").unwrap();
        assert_eq!(spec.modifiers, MOD_CONTROL | MOD_ALT);
        assert_eq!(spec.key, b'V' as u32);
        for key in [
            "A",
            "Z",
            "0",
            "9",
            "F1",
            "F24",
            "Space",
            "Tab",
            "Enter",
            "Escape",
            "Backspace",
            "Delete",
            "Insert",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "Left",
            "Right",
            "Up",
            "Down",
        ] {
            assert!(
                HotkeySpec::parse(&format!("Win+Shift+{key}")).is_ok(),
                "{key}"
            );
        }
    }

    #[test]
    fn invalid_combinations() {
        for value in [
            "",
            "V",
            "F8",
            "Ctrl",
            "Ctrl+Alt",
            "Ctrl+Ctrl+V",
            "Ctrl+ctrl+V",
            "Ctrl++V",
            "+V",
            "Ctrl+",
            "Ctrl+A+B",
            "Super+V",
            "Ctrl+Unknown",
            "Alt+F12",
            "Ctrl+F0",
            "Ctrl+F25",
            "Ctrl+F01",
            "Ctrl+é",
        ] {
            assert!(HotkeySpec::parse(value).is_err(), "{value}");
        }
    }
}
