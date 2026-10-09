## 1. 音乐库存储

- [x] 1.1 建立内部 muse-library crate，定义歌曲、附件、导入结果及错误类型，提供 SQLite 库打开和结构版本校验。
- [x] 1.2 实现元数据列表与按歌曲标识打开完整音频的独立操作，保证音频来源拥有自身生命周期。
- [x] 1.3 实现按源文件和最终音频 SHA-256 去重的单曲事务，以及删除、关联清理和重新导入。

## 2. 本地导入

- [x] 2.1 从同一份输入字节识别 MP3/FLAC 并读取标签，缺失信息按规格处理，不改写来源。
- [x] 2.2 使用 Rust 实现带边界检查的 NCM 解包及本地元数据读取，入库保持完整 MP3/FLAC 字节。
- [x] 2.3 实现递归目录扫描、文件级失败继续、库级错误停止、取消与结果汇总。
- [x] 2.4 保存嵌入和目录附件，限定同名 LRC 与歌曲 ID 封面的查找范围，重复导入只补齐缺失附件并报告附件问题。

## 3. 终端导入流程

- [x] 3.1 接入 Ratatui/Crossterm，显示空库或歌曲列表、状态栏与命令行，提供 j/k、:import、Esc 和 :q。
- [x] 3.2 通过独立导入任务保持界面可交互，显示完整结果及失败原因，退出时等待正在处理的单曲提交或回滚并恢复终端。
- [x] 3.3 增加 --library 路径与帮助/版本入口，文档明确实际功能；后续 native-playback 已接入内置播放。

## 4. 当前阶段验证

- [x] 4.1 验证重启与原文件移走、两层去重、NCM、附件补齐、删除重导入、事务失败与新版本拒绝等行为。
- [x] 4.2 运行 workspace CI、文档测试与 OpenSpec 严格校验，验证实际本地样本及终端退出流程。

## 5. Built-in playback (completed in native-playback)

- [x] 5.1 Implement the authorized Rust backend, define recovery/completion behavior and verify MP3/FLAC decoding, output, seeking and cleanup.
- [x] 5.2 Connect selection, pause/resume, seeking, completion and failures; verify boundaries, paused intent, stale requests, playback during import and quit.

Groups 1–4 established import and browsing. The user subsequently authorized group 5; native-playback implements and verifies the real backend. Decisions and evidence are recorded in its design.md. The earlier trait-only restriction describes the prior phase.
