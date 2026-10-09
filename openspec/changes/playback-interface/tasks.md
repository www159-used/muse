## 1. Workspace 与播放接口

- [x] 1.1 建立 muse-playback 库成员和根包 path 依赖，沿用工具链及许可证，并限定库不独立发布。
- [x] 1.2 声明 AudioSource、Playback、PlaybackState 与 PlaybackStatus，写明所有权、控制和错误约定，不加入后端实现。

## 2. 工程接入与验证

- [x] 2.1 调整 Makefile 与 CI 以覆盖 workspace 及接口文档示例，更新 README 和需求讨论中的播放决策。
- [x] 2.2 运行 workspace 检查、文档示例编译、OpenSpec 校验，并在隔离仓库验证发布预演不为内部库独立发布。

## 3. 时间定位接口声明

- [x] 3.1 声明 SeekTarget 与必需的 Playback::seek，公开文档说明实际落点、执行时相对位置、边界、状态和失败约定，更新 Finished 定义，不增加后端或默认实现。
- [x] 3.2 补充绝对跳转、快进与快退的 trait object 使用示例，运行 workspace、文档编译和 OpenSpec 检查，不将编译成功视为实际定位验证。

第 1、2 组完成项覆盖原有五个基础方法的声明与编译验证；第 3 组已完成定位声明与示例验证。本轮 make ci 通过，包含 35 项现有测试、2 个文档示例及 release 构建；播放库文档在禁止警告时成功生成，OpenSpec 严格校验通过。design.md 记录后续推荐的 NativePlayback、Symphonia/Rodio 分工、有限 PCM 缓冲及候选持续故障恢复，未计作已应用或已验证；确定后需在后续工作中同步公开 Rust 文档与规格。真实后端及播放行为验证仍不属于本变更的已完成任务。
