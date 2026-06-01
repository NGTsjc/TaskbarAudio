# TaskbarAudio
Windows 11 任务栏音频频谱可视化（WASAPI loopback + Direct2D）。程序会尝试把渲染面板嵌入到任务栏；若失败则降级为仅托盘模式，并提供“重试附加”。
## 环境要求
- Windows 11（建议 22H2+）
- Rust（本项目使用 edition 2024；建议 Rust 1.85+，仓库当前使用 1.96.0 开发）
- MSVC 工具链：Visual Studio 2022 Build Tools（包含“使用 C++ 的桌面开发”）与 Windows 10/11 SDK
## 构建
- Debug：
  - `cargo build`
  - 产物：`target\debug\taskbar_audio.exe`
- Release：
  - `cargo build --release`
  - 产物：`target\release\taskbar_audio.exe`
## 运行
- 运行可执行文件（Debug/Release 均可）：`.\target\release\taskbar_audio.exe`
- 启动后应出现托盘图标
- 右键托盘图标：
  - “退出”：退出程序
  - “重试附加”：当嵌入任务栏失败而处于降级模式时可见，用于再次尝试内嵌
## 最小可重复验证步骤（手动）
1. Release 构建：`cargo build --release`
2. 运行：`.\target\release\taskbar_audio.exe`
3. 确认托盘图标出现
4. 播放任意有声内容（浏览器视频/音乐），观察任务栏上的频谱柱体随音频变化
5. 若未出现频谱面板：
   - 先确认托盘模式正常（有托盘图标与菜单）
   - 右键托盘图标，点击“重试附加”
6. Win+D 验证：按下 Win+D 显示桌面，确认面板仍随任务栏可见（必要时等待 1 秒自恢复）
7. 空闲占用验证：停止播放音频并等待约 10 秒，确认日志中出现 `runtime stats` 且 `timer_ms` 降至 120/250/500（空闲/不可见降帧）
8. 鼠标穿透验证：在频谱区域点击/滑动不应影响任务栏按钮交互（不抢焦点）
9. 右键托盘图标点击“退出”，确认进程退出且任务栏恢复正常
## 日志与自检
- 日志文件：`%LOCALAPPDATA%\TaskbarAudio\logs\taskbar_audio.log`
- 启动自检点：
  - 看到 `app started` 表示消息循环已启动
  - 若发生异常退出，日志中会记录 `fatal error`；若发生 panic，会记录 `panic` 与源码位置
- 提升日志级别（PowerShell）：
  - `$env:RUST_LOG="debug"; .\target\release\taskbar_audio.exe`
## 构建故障排查（受限环境）
部分受限/沙箱环境会限制子进程创建，导致依赖 crate 的 build script 无法执行 `rustc --version` 或编译探测，从而在构建时出现类似错误：`Os { code: 0, message: "操作成功完成。" }`。此时需要在允许子进程创建的本机开发环境中构建，或放开对应限制后再执行 `cargo build --release`。


方式 1：启动文件夹（最简单）

- Win + R 输入： shell:startup
- 把 target\release\taskbar_audio.exe 的 快捷方式 放进去（建议放快捷方式，不要直接拷 exe）
- 下次登录系统会自动启动