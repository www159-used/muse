## Why

播放控制需要独立于终端界面、音乐库存储和 NCM 导入。用户确认将播放作为 workspace 内的库子项目，并要求当前仅提供 trait，具体播放实现以后再接入。

## What Changes

- 保留根目录的 muse 二进制包，增加 workspace 成员 `crates/muse-playback`。
- 播放库基础接口只使用标准库，公开音频来源、播放控制 trait 和状态快照类型；后续 native-playback 变更提供可选内置实现。
- 定义加载、播放、暂停、停止、状态读取的职责和错误约定，不提供默认成功实现或播放后端。
- 声明时间定位接口：以 SeekTarget 区分绝对时间、向前移动和向后移动，一个 seek 方法返回实际落点，公开文档说明状态、边界和失败约定。
- 后续实现采用内置 Rust 播放方向，本次不引入音频依赖或 mpv。
- CI 与 Makefile 检查覆盖整个 workspace，保持现有 Git 标签与二进制发布方式。
- 后续设计推荐 `NativePlayback` 作为内置实现，采用 Symphonia 解码和 Rodio 输出；模块职责、有限缓冲、加载替换及失败恢复记录在 design.md，当前不引入依赖或实现。

## Capabilities

### New Capabilities

- `playback-interface`: 独立播放库的音频来源、控制接口、状态与作用域。

### Modified Capabilities

无。播放器功能仍由 `muse-mvp` 讨论，本变更只建立接口。

## Impact

- 调整 Cargo workspace、锁文件和 workspace 检查命令，增加标准库依赖的播放接口 crate。
- 播放库沿用工程工具链与 AGPL-3.0-only 许可证，不发布到 crates.io。
- 本接口变更负责声明；后续 native-playback 已接入解码器和设备，应用默认启用，基础接口仍可独立构建。
- 已有接口声明的成功状态转换与后续推荐架构、待确定的失败恢复方案在 design.md 中分开记录；候选补充不表示公开契约已经更新或实际播放已经验证。
- 当前 Rust 已声明 SeekTarget 和第六个控制方法 seek；使用示例通过编译检查，接口声明不表示后端或终端已支持定位。
