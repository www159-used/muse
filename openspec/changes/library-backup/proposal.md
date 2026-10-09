## Why

音乐库保存完整音频、附件和去重记录，需要能够在升级前保存一致副本并从副本恢复。当前采用 WAL，直接复制运行中的主数据库文件不能保证包含所有已提交内容。

## What Changes

- 增加存储库的一致备份与恢复 API，保存歌曲、音频、附件、成功导入记录和结构版本。
- 本轮实现选择 SQLite Online Backup API；完成检查后才将临时文件发布到指定路径，已有目标不覆盖。
- 恢复创建新的音乐库文件，拒绝外部数据库、更高结构版本和完整性检查失败的副本。
- 增加无需交互终端的 `--backup <目标>` 与 `--restore <来源>`，通过 `--library` 指定源库或恢复目标；缺失源文件不创建空库。
- 验证 WAL 未检查点内容、未提交事务、附件与去重记录、失败清理及命令行行为。

这些是备份阶段的实现选择。自动迁移前备份、覆盖原库恢复和定时备份不属于此次变更；播放后端另由 native-playback 变更接续。

## Capabilities

### New Capabilities
- `library-backup`: 音乐库的一致单文件备份、校验及恢复到新路径，以及对应非交互命令行入口。

### Modified Capabilities

无。现有导入、读取和删除语义保持有效。

## Impact

影响 `crates/muse-library`、根程序参数解析、README 和测试。启用已有 rusqlite 的 backup feature，复用 tempfile 管理同目录临时文件，不新增数据库结构版本或播放实现。
