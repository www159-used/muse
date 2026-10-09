## ADDED Requirements

### Requirement: 独立播放接口 crate

项目 SHALL 保留根目录 muse 二进制包，并提供 workspace 成员 `muse-playback` 作为库 crate。播放库 SHALL 沿用项目工具链和 AGPL-3.0-only 许可证。不启用 native feature 的基础接口 SHALL 只依赖标准库，不包含具体播放后端、默认成功实现或后端构造器；可选内置实现由 native-playback 规格定义。

#### Scenario: 构建 workspace
- **WHEN** 开发者以不启用默认 feature 的方式单独构建 muse-playback 基础接口
- **THEN** 基础接口成功编译，无需安装解码器、音频设备开发库或 mpv；应用的 native 构建遵循 native-playback 要求

### Requirement: 声明播放控制与音频来源

播放库 SHALL 提供音频来源 trait，要求可读取、可定位、可转移线程且拥有自身生命周期。播放控制 trait SHALL 声明加载、播放、暂停、停止、时间定位、状态读取方法，并以标准错误 trait 约束的关联类型返回错误。控制方法 SHALL 不提供默认实现，接口 SHALL 不包含数据库或界面类型。

#### Scenario: 调用方使用 trait object
- **WHEN** 调用方通过指定错误类型的播放 trait object 声明控制函数
- **THEN** 代码可编译，并可调用基础控制方法，无需指定解码或设备类型

#### Scenario: 使用标准读取器作为输入
- **WHEN** 调用方将拥有自身生命周期的文件或内存游标包装成音频来源 trait object
- **THEN** 代码可编译，输入以所有权转移方式交给播放控制接口

### Requirement: 声明播放状态契约

播放库 SHALL 提供状态快照，包含 Empty、Ready、Playing、Paused、Finished 状态、当前位置和可缺省总时长。接口文档 SHALL 说明成功加载不自动播放、暂停保留位置、停止归零、自然结束由调用方决定后续动作，以及错误由实现返回。当前变更 SHALL 只声明这些约定，不实现或声称已经验证播放行为。

#### Scenario: 阅读播放控制接口
- **WHEN** 开发者查看播放 trait 的公开文档
- **THEN** 能确认输入所有权、基本状态转换、错误返回与调用顺序，不会把接口声明误认为已有播放实现

### Requirement: 声明时间定位接口与契约

播放库 SHALL 声明 SeekTarget，使用 Duration 分别表达绝对时间、向前移动和向后移动。Playback SHALL 增加必需的 seek 方法，返回 Result<Duration, Self::Error>，不提供默认实现。接口文档 SHALL 将成功返回值定义为完成定位时的实际落点，说明相对移动由实现按操作执行时的位置计算，而不是让调用方先读取状态计算目标。本要求只覆盖接口声明与使用示例编译，当前变更 SHALL 不实现或声称已经验证定位行为。

接口文档 SHALL 说明：负向越过开头截取到零；可靠总时长已知时越过结尾截取到总时长；时长未知时不能以估计标签伪造边界，不支持或无法完成定位时返回错误。成功定位在曲内时，Playing 保持播放、Paused 保持暂停；Ready 定位至非零位置以及 Finished 定位回曲内成为 Paused；成功定位到已确认曲终成为 Finished，由调用方决定后续动作。

接口文档 SHALL 区分执行前拒绝与执行中故障：无来源或不支持定位不能假报成功；前置拒绝保留正常旧来源与原播放意图，正在播放的位置可继续自然推进；执行中读取、解码或输出故障不能承诺回滚到精确旧位置。已有持续故障的恢复契约仍按 design.md 的候选方案处理，不因增加定位声明而成为已经验证的后端行为。

#### Scenario: 通过播放 trait object 表达三种定位
- **WHEN** 调用方通过指定错误类型的播放 trait object 表达绝对跳转、快进或快退
- **THEN** 三种 SeekTarget 均可传给同一 seek 方法，并通过结果取得实际位置或错误，无需指定后端类型

#### Scenario: 阅读定位状态与边界约定
- **WHEN** 开发者查看时间定位的公开文档
- **THEN** 能确认实际落点、相对移动基准、开头结尾边界、暂停保留及曲终处理，不会把输入读取器的字节 Seek 当成音频时间定位

#### Scenario: 阅读定位失败约定
- **WHEN** 开发者查看定位失败的公开文档
- **THEN** 能区分无来源或不支持的前置拒绝与定位开始后的故障，不会将失败报告为成功或依赖精确旧位置回滚

### Requirement: workspace 检查与内部库发布范围

CI 和 Makefile SHALL 检查整个 workspace 的格式、Clippy、测试与 release 构建，并编译接口使用示例。播放库 SHALL 不独立发布到 crates.io 或生成 Git 标签，应用 SHALL 保持现有版本标签与二进制发布方式。

#### Scenario: 执行本地检查
- **WHEN** 开发者执行 make ci
- **THEN** 检查覆盖主程序及播放库，并验证公开接口示例可编译

#### Scenario: 预演应用发布
- **WHEN** 在已提交的隔离仓库中预演应用补丁发布
- **THEN** 应用沿用现有版本标签格式，播放库不独立产生发布或标签操作
