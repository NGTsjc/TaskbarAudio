# Taskbar Audio Visualizer

A Windows taskbar audio visualizer component.

## Features

- Real-time audio visualization in the taskbar
- Supports bar and line chart display modes
- Configurable position (left, center, right)
- Horizontal offset adjustment
- Mixed audio capture (system + microphone)
- Global hotkey and tray actions to pause/hide or resume/show the visualizer

## Requirements

- Windows 10 or later
- Rust toolchain (for building)

## Build

```bash
cargo build --release
```

`Cargo.lock` pins the application dependencies. CI uses `--locked`, runs tests
before creating a tag release, and checks normal `master` pushes and pull requests.

## Development checks

CI installs the latest stable Rust/Clippy. Run `rustup update stable` before
checking locally to avoid differences in lint rules between toolchain versions.

```powershell
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
cargo build --release --locked
# Requires working Windows playback and microphone devices:
cargo test --offline live_system_and_mixed_capture_restart -- --ignored --nocapture
# Interactive desktop; exit the running visualizer first:
cargo test --offline desktop_hotkey_tray_resume_and_restart -- --ignored --nocapture
```

The regular tests cover shortcut/config parsing, audio decoding and mixing,
thread shutdown, FFT output, panel lifecycle, and bar/line pixel bounds at multiple
DPI settings. The optional device test starts and stops both capture modes twice.
The optional desktop test checks hotkey registration, message routing, tray
double-click resume, panel bounds, recovery after panel loss, and restart while paused.

## Configuration

Create a `config.json` file in the same directory as the executable:

```json
{
    "position": "center",
    "display_mode": "bar",
    "offset_x": 0,
    "environment_volume": false,
    "auto_start": false,
    "toggle_hotkey": ""
}
```

### Options

- `position`: `left`, `center`, `right`
- `display_mode`: `bar`, `line`
- `offset_x`: Horizontal offset in pixels (positive = right, negative = left)
- `environment_volume`: Enable mixed audio capture (system + microphone). Windows converts microphone audio to the system audio format before mixing. Changes to either default device restart capture.
- `auto_start`: Enable Windows auto start via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
- `toggle_hotkey`: Optional global shortcut, for example `"Ctrl+Alt+V"`. Omit it, use `null`, or leave it empty to disable. Configuration changes take effect after restart. Both flat configuration and options nested inside `"user"` are supported.

UTF-8 files with or without a BOM are supported. Malformed JSON or invalid option
types fall back to defaults with a warning; an invalid `toggle_hotkey` value only
disables the shortcut and preserves the other options.

### Hide, pause, and resume

The visualizer starts enabled on every launch. Press the configured shortcut or select **隐藏音频** (Hide audio) in the tray menu to pause it. Pausing stops audio capture (including the microphone in mixed mode), device polling, spectrum analysis, rendering, and taskbar maintenance timers. Only the tray, shortcut, and Windows message loop remain active.

Press the shortcut again, select **显示音频** (Show audio), or double-click the tray icon while paused to resume. Resume recreates the visualizer using the current default audio devices. If the taskbar is unavailable, attachment retries continue until it becomes available. Visibility is not saved between launches. Startup and resume check taskbar geometry every 50 ms and show the panel after it has remained stable for at least 100 ms. The panel width is calculated from the bar count, with 5-pixel columns, 2-pixel gaps, and 6-pixel padding on each side.

### Shortcut rules

Shortcuts are case-insensitive and allow whitespace around `+`. Use one or more distinct modifiers (`Ctrl`, `Alt`, `Shift`, `Win`), followed by exactly one main key:

- `A`–`Z`, `0`–`9`, or `F1`–`F24` (except Windows-reserved `F12`).
- `Space`, `Tab`, `Enter`, `Escape`, `Backspace`, `Delete`, `Insert`, `Home`, `End`, `PageUp`, `PageDown`, `Left`, `Right`, `Up`, `Down`.

Single keys, duplicate modifiers, empty parts, unknown keys, and multiple main keys are rejected. Invalid values (including incorrect JSON value types) disable only the shortcut. Shortcuts already occupied or rejected by Windows are ignored with a warning; tray controls continue working. Holding the shortcut does not repeatedly toggle the visualizer.

## Application icon

`asset/logo.ico` is embedded into the executable as `MAINICON` and is also used
by the tray. It combines the audio waveform with a smaller personal mark on a
transparent background, with eight sizes from 16 to 256 pixels.

After replacing it, run `cargo build --release --locked` and distribute the
rebuilt `target/release/taskbar_audio.exe`. Existing GitHub release archives keep
their original icon; publishing the new icon requires committing the change and
pushing a new version tag. Updating the PowerShell `$tag` variable alone does not
change the package version in `Cargo.toml`.

### Explorer still shows the old icon

If the tray and Task Manager show the new icon but File Explorer shows the old
one, refresh the Windows icon cache and reopen the folder:

```powershell
ie4uinit.exe -show
```

If necessary, restart **Windows Explorer** from Task Manager or follow Microsoft's
[icon cache rebuilding instructions](https://learn.microsoft.com/en-us/troubleshoot/windows-client/shell-experience/application-shortcuts-show-blank-icons).

## License

MIT
