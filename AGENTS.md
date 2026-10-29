# ADB 工作台

遵守上一层项目约定。用户确认各功能单位后再做本地 Git 提交。

- 后端：Rust / Axum / Tokio / SeaORM / SQLite；前端：SvelteKit 静态网页。
- 修改工作台前端时，使用项目级 [前端开发规范](.agents/skills/adb-studio-frontend/SKILL.md)。
- 任务仅通过工作流编辑器创建或导入，不恢复旧内置任务、旧动作或裸本体入口。使用 `scripts/studio.py` 管理工作流；接口见 `docs/workflows.md`。每项只保留一份当前内容，保存后直接运行，样本检查为可选诊断。
- 所有手机命令经 `src/adb.rs` 核对自身编号。不要绕过同机任务互斥或恢复检查。
- `data/` 保存运行状态、令牌、截图及日志，不提交；不得记录模型密钥。
- 清理旧任务前备份数据库，保留编辑器任务、草稿、手机和设置。
- 修改执行行为时运行 `cargo test` 与模拟设备集成检查。网页修改运行 `npm run check` 和构建。
- 以真实失败证据调整规则；保持用户说明、接口文档与实际行为一致。
- Windows 安装与自启见 `docs/deployment.md`；Mac 使用 launchd，Linux 使用 systemd，平台分支不得混用。Windows 模拟检查通过 `scripts/check_support.py` 调用 Python 模拟设备，不操作真机。

## 当前状态与下一步

画布操作录制已通过用户验收并更新 Mac 正式服务 17841；范围与验收步骤见 `docs/workflow-recording.md`。

Windows 本机部署、模拟检查及用户真机 Wi-Fi 连接、截图、投屏验收通过。安装、启动、停止、自启和 ZIP 使用见 `docs/deployment.md`。待完成：Mac 完整实机回归与完整 Linux 后台回归。

投屏网页亮度调节已通过用户验收；Mac 本地部署已通过用户验收。Windows 的该项部署状态仍待确认。使用说明见 `docs/ui-components.md` 的设备右栏章节；Mac 部署记录见 `docs/changes/2026-10-29-Mac更新本地工作台.md`。

Windows 正式服务 17841 已部署变量显示别名与紧凑运行参数表单，用户验收通过；见 `docs/changes/2026-10-29-增加变量显示别名.md`。工作流运行变量、模型优先条件判断与重启 ADB 已部署；此前部署核对及模型时限检查限制见 `docs/changes/2026-10-29-Windows更新本地工作台.md`。

连接点添加节点与移除左侧步骤库已通过用户验收，部署至 Windows 正式服务 17841。见 `docs/changes/2026-10-29-连接点添加工作流节点.md`。

变量紧凑列表、添加／编辑弹窗与取消变量单位已通过用户验收，部署至正式服务。单位由引用节点字段决定，旧数值与引用保留；见 `docs/changes/2026-10-29-变量弹窗与紧凑列表.md` 与 `docs/changes/2026-10-29-变量仅保存数值.md`。

工作流副本导入已通过用户验收并部署，新建／导入按钮放在任务页右上角；覆盖导入待开发。当前接口与方案见 `docs/workflow-import-export.md`。

条件判断与等待条件的模型优先规则及未可靠确认走不符合已通过用户验收并部署至本地正式服务；直接填写 Key／Base URL 等设置调整仍待实施，见 `docs/recognition-fallback-plan.md`。

正式工作台 17841 仅保留编辑器工作流，旧任务清理已通过用户验收并部署；现有工作流、草稿、手机与凭据保留。部署及检查记录见 `docs/changes/2026-10-28-移除旧任务.md`。
