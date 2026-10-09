## Context

项目尚无 Cargo 工程或 Git 仓库。用户已确认 Rust，并要求 CI、cargo-release、AGPL 官方许可证、工具链文件和 Makefile；发布范围确认为 Git 标签与二进制。

## Goals / Non-Goals

**Goals:** 提供可构建的最小 Rust 工程，统一开发检查和版本发布入口，确保发布包附带许可证。

**Non-Goals:** 当前不实现播放器、音乐库或 NCM FFI，不发布到 crates.io，不在此次初始化中提交、创建版本标签或推送远端。

## Decisions

### 已确认：Rust、AGPL 与二进制发布

使用 Rust 2024 edition。工具链固定为本机已有的 1.95.0，包含 rustfmt 和 Clippy；锁文件纳入版本管理。许可证按 AGPL-3.0-only 配置，LICENSE 保存 GNU 官方全文，README 说明许可证来源。Cargo 包设置 `publish = false`。

### 实施选择：本地命令与 CI 保持一致

Makefile 提供格式化、格式检查、Clippy、测试、开发构建、release 构建与完整检查命令。CI 在 Linux、macOS 和 Windows 上使用工具链文件，运行同一组 Cargo 检查。

### 实施选择：cargo-release 管理版本，GitHub Actions 分发二进制

锁定 cargo-release 1.1.2，限制从 main 分支发布，使用 `v{{version}}` 标签。默认预演；显式执行才更新版本和创建本地 Git 标签，自动推送关闭。发布前运行完整检查。

版本标签推送后，工作流验证标签与 Cargo 版本一致，分别为 Linux x86_64、macOS ARM64 和 Windows x86_64 构建。发布归档包含二进制、LICENSE 和 README，附带 SHA-256 校验文件，汇总到 GitHub Release。Actions 引用固定完整提交 SHA。

## Risks / Trade-offs

- 初始化时尚无 Git 远端或初始提交 → 完成配置和本地验证，实际 GitHub 工作流须在仓库上传后运行。
- 工具链更新可能影响本地和 CI → 使用固定版本，升级时同时检查开发与发布流程。
- 标签与 Cargo 版本可能不一致 → 打包前拒绝不匹配的标签。
- 原生依赖尚未引入 → 当前发布矩阵只覆盖工程骨架，后续 FFI 接入时重新验证链接和分发。

## Migration Plan

在当前目录初始化本地 main 分支，添加工程文件并运行检查。无需迁移音乐数据。

## Open Questions

GitHub 远端尚未指定，接入仓库后才能观察实际远端 CI 和 Release 结果。
