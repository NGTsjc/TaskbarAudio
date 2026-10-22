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
- `environment_volume`: Enable mixed audio capture (system + microphone)
- `auto_start`: Enable Windows auto start via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
- `toggle_hotkey`: Optional global shortcut, for example `"Ctrl+Alt+V"`. Omit it, use `null`, or leave it empty to disable. Configuration changes take effect after restart. Both flat configuration and options nested inside `"user"` are supported.

### Hide, pause, and resume

The visualizer starts enabled on every launch. Press the configured shortcut or select **隐藏音频** (Hide audio) in the tray menu to pause it. Pausing stops audio capture (including the microphone in mixed mode), device polling, spectrum analysis, rendering, and taskbar maintenance timers. Only the tray, shortcut, and Windows message loop remain active.

Press the shortcut again, select **显示音频** (Show audio), or double-click the tray icon while paused to resume. Resume recreates the visualizer using the current default audio devices. If the taskbar is unavailable, attachment retries continue until it becomes available. Visibility is not saved between launches. Startup and resume check taskbar geometry every 50 ms and show the panel after it has remained stable for at least 100 ms. The default spectrum uses 40 bars across a 290-pixel panel, with 5-pixel columns and 2-pixel gaps.

### Shortcut rules

Shortcuts are case-insensitive and allow whitespace around `+`. Use one or more distinct modifiers (`Ctrl`, `Alt`, `Shift`, `Win`), followed by exactly one main key:

- `A`–`Z`, `0`–`9`, or `F1`–`F24` (except Windows-reserved `F12`).
- `Space`, `Tab`, `Enter`, `Escape`, `Backspace`, `Delete`, `Insert`, `Home`, `End`, `PageUp`, `PageDown`, `Left`, `Right`, `Up`, `Down`.

Single keys, duplicate modifiers, empty parts, unknown keys, and multiple main keys are rejected. Invalid values (including incorrect JSON value types) disable only the shortcut. Shortcuts already occupied or rejected by Windows are ignored with a warning; tray controls continue working. Holding the shortcut does not repeatedly toggle the visualizer.

## License

MIT
