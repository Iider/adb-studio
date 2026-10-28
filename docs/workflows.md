# 工作流与接口

任务只通过工作流编辑器创建。旧内置点击、滑动、汽水及旧纯视觉任务已移除，不再转换或恢复。工作流内的点击、滑动、按键、等待、条件、循环与定时继续使用。

## 创建和保存

网页“自动化任务”右上角提供“创建新工作流”和“导入工作流文件”。草稿自动保存未完成的编辑；手动“保存”检查连线、标注与参数，生成可运行任务。保存覆盖当前任务，已启动的运行保留启动时的内容。

命令工具通过 `data/service-root` 查找本地服务与访问凭据，不要输出令牌。接口路径在命令中省略 `/api`。

```sh
python scripts/studio.py devices
python scripts/studio.py workflows
python scripts/studio.py request GET /workflow-drafts
```

| 接口 | 用途 |
| --- | --- |
| `GET /api/workflow-drafts` | 列出草稿 |
| `GET /api/workflow-drafts/{UUID}` | 读取草稿 |
| `PUT /api/workflow-drafts/{UUID}` | 保存草稿；新建 revision=0，修改带读取到的 revision |
| `POST /api/workflow-drafts/{UUID}/save` | 请求 `{"revision":当前编号}`，检查并覆盖可运行任务 |
| `DELETE /api/workflow-drafts/{UUID}?revision=N` | 删除未关联已保存任务的草稿；关联任务须从任务列表整体删除 |
| `GET /api/workflows` | 列出已保存任务 |
| `DELETE /api/workflows/{id}` | 删除任务、草稿、定时、全部历史记录与截图 |
| `GET /api/editor/observe/{device_id}` | 取得编辑器使用的当前画面与屏幕信息 |

并发修改草稿返回 409，不自动覆盖另一页面的编辑。`POST /api/workflows` 已关闭；不能直接提交旧任务本体。正在运行、等待、恢复或暂停的任务须先停止再删除。

## 草稿与工作流格式

草稿字段为 `id`（UUID）、`revision`、`name`、`description`、`entry`、`nodes`、`viewport`。每个节点含 `id`、`kind`、`label`、`config`、`targets`、`position`。`viewport` 为 `[横向位置,纵向位置,缩放]`；节点位置为 `[x,y]`。

可用节点：`tap`、`swipe`、`back`、`home`、`recents`、`wait`、`condition`、`wait_condition`、`loop`、`break`、`continue`、`end`。各项设置、手机标注和连线见[编辑器说明](visual-workflow-editor.md)。条件的页面依据可留空；读字失败与陌生页面不会自动成为“不包含”。

保存后编号为 `manual-<草稿UUID>`。可运行内容只有 `canvas` 和 `end` 两类内部动作，不能混入旧动作。画布草稿和已保存内容分别保留；不能用未保存草稿替换正在运行或导出的任务本体。

当前文字条件需要本地读字工具。模型替代读字和直接填写 Key 仍待实施，见[整改方案](recognition-fallback-plan.md)。无需文字判断的点击、滑动和等待不依赖本地读字或模型。

## 运行与定时

```sh
python scripts/studio.py run manual-草稿UUID --device 手机自身编号
python scripts/studio.py runs
python scripts/studio.py pause 运行编号
python scripts/studio.py resume 运行编号
python scripts/studio.py stop 运行编号
```

`POST /api/runs` 只接收 `device_id`、`workflow_id`；不接收旧 tap／swipe 专用参数。次数、间隔和位置在工作流编辑器设置。

`GET /api/runs/{id}/events` 读取事件；`GET /api/runs/{id}/export` 导出执行证据，不能作为工作流导入。`DELETE /api/runs/{id}` 清理已结束记录，运行截图仍留在本机归档；删除整个任务才同时清理其全部截图。

`GET/POST /api/schedules` 查询或新增定时；`PUT/DELETE /api/schedules/{id}` 修改或删除。新增与修改字段为 `name`、`device_id`、`workflow_id`、`enabled`、`rule`，执行保存的工作流内容。规则支持 `once`（at）、`daily`（time）、`weekly`（time、days）、`interval`（minutes），界面时间统一使用上海时区。

同机一次一个任务；暂停仍占用手机，定时遇到忙碌或离线会跳过。运行途中掉线等待；重启保留人工暂停。继续前核对手机、锁屏和原应用；已发出但结果不明的动作拒绝重放。

## 工作流文件导出

任务卡片“更多 → 导出工作流”导出本体和关联草稿。编辑器“更多 → 导出当前草稿”只导出草稿；未完成的编辑也可携带。

- `GET /api/workflows/{id}/export`
- `GET /api/workflow-drafts/{UUID}/export?revision=N`

文件格式为 `adb-studio.workflow`、`format_revision: 1`。`content.workflow` 为可运行内容或 null，`content.draft` 必须包含画布草稿；`assets` 携带引用的 PNG。草稿仅携带编辑字段，不包含服务端 revision／updated_at。本体与草稿有差异时分别保留，并在下载前提示。

响应 `X-Workflow-Draft-State` 为 saved／modified／incomplete／draft。缺少任务或草稿返回 404，草稿编号冲突返回 409，非法图片与引用返回 400，超限返回 413。

## 导入工作流副本

任务页右上角选择文件，预览后点“导入副本”。只接受编辑器导出的工作流文件或草稿文件；拒绝裸本体、旧任务动作、无草稿文件。不支持自动转换。

`POST /api/workflow-import/preview` 与 `POST /api/workflow-import` 请求：

```json
{"operation_id":"本次操作UUID","file":{},"name":"可选的新名称"}
```

预览不写入。导入生成新草稿 UUID，任务编号为 `manual-<新UUID>`，保留画布、标注、内部连线及草稿与本体的差异；不运行、不创建定时。仅草稿文件需手动保存后才能运行。重复导入可创建独立副本；同一次请求重试使用相同 operation_id，避免重复写入。同编号不同内容返回 409；原副本已删除则返回 removed=true，不重建。

请求最多 32 MiB；每份本体或草稿最多 `2 MiB - 1024 字节`；最多 40 张 PNG，原始图片合计最多 20 MiB，单张最多 8 MiB、宽高各最多 8192。格式、坏图、缺图、重复附件、路径和旧动作返回 400。写入失败回退本次内容，重启按清单清理无引用图片。文件不携带模型密钥、令牌、运行记录或定时。

## 可选样本检查

`POST /api/workflows/{id}/validate` 检查编辑器条件样本，返回诊断结果；不阻止无样本任务保存或运行。当前不执行旧纯视觉任务的图片分支检查。独立模型看图检查见[识别说明](vision-recognition.md)。

## 旧数据清理

首次启动读取旧任务原始记录，在转成可运行内容前删除旧本体、关联定时、执行记录和事件，按清单清理运行截图。清理前在同一 data 目录生成 `before-editor-only-<UUID>.sqlite` 备份；备份失败则中止清理。工作流编辑器创建的任务、草稿、手机与模型设置保留。新启动不再预置任何任务。
