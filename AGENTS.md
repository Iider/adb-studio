# ADB 工作台

遵守上一层项目约定。用户确认各功能单位后再做本地 Git 提交。

- 后端：Rust / Axum / Tokio / SeaORM / SQLite；前端：SvelteKit 静态网页。
- 使用 `scripts/studio.py` 管理工作流；格式见 `docs/workflows.md`。版本不可覆盖，发布需样本与真机试跑通过。
- 所有手机命令经 `src/adb.rs` 核对自身编号。不要绕过同机任务互斥或恢复检查。
- `data/` 保存运行状态、令牌、截图及日志，不提交；不得记录模型密钥。
- 修改执行行为时运行 `cargo test` 与模拟设备集成检查。网页修改运行 `npm run check` 和构建。
- 以真实失败证据调整规则；保持用户说明、接口文档与实际行为一致。
