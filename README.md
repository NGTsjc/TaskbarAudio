# Taskbar Audio Visualizer

A Windows taskbar audio visualizer component.

## Features

- Real-time audio visualization in the taskbar
- Supports bar and line chart display modes
- Configurable position (left, center, right)
- Horizontal offset adjustment
- Mixed audio capture (system + microphone)

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
    "environment_volume": false
}
```

### Options

- `position`: `left`, `center`, `right`
- `display_mode`: `bar`, `line`
- `offset_x`: Horizontal offset in pixels (positive = right, negative = left)
- `environment_volume`: Enable mixed audio capture (system + microphone)

## Auto Start

1. Press `Win + R`
2. Enter: `shell:startup`
3. Create a shortcut to `taskbar_audio.exe` and place it in the startup folder

## License

MIT