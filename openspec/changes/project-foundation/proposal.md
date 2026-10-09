## Why

muse 已确认使用 Rust，但尚无可构建的项目骨架和工程流程。用户要求补齐 CI、cargo-release、AGPL 官方许可证、固定工具链和 Makefile，为后续播放器开发建立基础。

## What Changes

- 初始化 Cargo 二进制项目与锁文件，固定 Rust 工具链并提供统一 Makefile 命令。
- 使用 AGPL-3.0-only，保存 GNU 官方许可证全文并填写 Cargo 许可证元数据。
- 配置 GitHub Actions，检查格式、Clippy、测试及构建。
- 配置 cargo-release 管理版本号和 Git 标签；推送版本标签后构建并发布二进制。
- 发布范围已由用户确认：Git 标签和二进制，不发布到 crates.io。

## Capabilities

### New Capabilities

- `project-tooling`: 可构建的 Rust 骨架、开发命令、许可证、CI 和二进制发布流程。

### Modified Capabilities

无，本变更不修改音乐库或播放器的产品需求。

## Impact

- 新增 Cargo 配置、Rust 入口、工具链文件、Makefile、许可证、开发说明及 GitHub Actions 工作流。
- 初始化本地 Git 仓库，以便 cargo-release 管理版本和标签。
- 本次完成工程基础，不提前实现音乐播放、SQLite 存储或 NCM FFI。
