## ADDED Requirements

### Requirement: 固定工具链与统一开发命令

项目 SHALL 提供可构建的 Cargo 二进制工程、Cargo.lock、固定版本的 rust-toolchain.toml 和 Makefile。开发者 SHALL 能通过 Makefile 执行格式、Clippy、测试与构建检查。

#### Scenario: 本地完整检查
- **WHEN** 开发者安装 rustup 和 make 并执行 `make ci`
- **THEN** 项目使用指定工具链执行格式检查、Clippy、测试及构建，任一步失败时返回非零状态

### Requirement: AGPL 官方许可证

项目 SHALL 使用 AGPL-3.0-only，并在 LICENSE 中提供 GNU 官方许可证全文；Cargo 元数据与发布归档 SHALL 保持相同许可证声明。

#### Scenario: 获取发布包
- **WHEN** 用户解压任一平台发布包
- **THEN** 包内同时包含 muse 二进制、LICENSE 与 README

### Requirement: 持续集成检查

项目 SHALL 在 GitHub Actions 中对推送和拉取请求执行格式、Clippy、测试和构建检查，并覆盖 Linux、macOS 和 Windows。

#### Scenario: 提交检查失败
- **WHEN** 某个平台的任一检查返回非零状态
- **THEN** 对应 CI 作业失败，检查结果可在工作流中查看

### Requirement: 版本与二进制发布

项目 SHALL 配置 cargo-release 管理版本与 `v` 前缀的 Git 标签，默认执行预演。版本标签推送后 SHALL 构建多平台二进制并发布至 GitHub Release，不向 crates.io 发布。

#### Scenario: 发布预演
- **WHEN** 开发者执行 Makefile 的版本发布预演命令
- **THEN** 显示计划执行的版本和标签步骤，不实际提交或推送

#### Scenario: 标签与版本不一致
- **WHEN** 发布标签中的版本与 Cargo.toml 的版本不一致
- **THEN** 打包失败，不进入 GitHub Release 发布步骤

#### Scenario: 正确版本标签
- **WHEN** 标签与 Cargo 版本一致且所有平台检查和构建成功
- **THEN** GitHub Release 提供三个平台的二进制归档与 SHA-256 校验文件
