# Agent 工作流接口

从项目目录运行 `python3 scripts/studio.py`。工具自动读取本机令牌；不要把令牌写进工作流、文档或模型提示。

## 常用命令

```sh
python3 scripts/studio.py devices
python3 scripts/studio.py request GET /devices/手机自身编号/screenshot --output /绝对路径/当前画面.png
python3 scripts/studio.py workflows
python3 scripts/studio.py runs
python3 scripts/studio.py request POST /workflows --file /绝对路径/工作流.json
python3 scripts/studio.py request POST /workflows/编号@版本/validate
python3 scripts/studio.py run 编号 --device 手机自身编号 --version 版本 --trial
python3 scripts/studio.py request GET /runs/运行编号/export --output /绝对路径/运行记录.json
python3 scripts/studio.py stop 运行编号
```

手机编号取 `devices` 返回的 `id`，不是无线地址。工作台会在实际操作前核对设备自身编号。

## 工作流格式

```json
{
  "schema_version": 1,
  "id": "short-swipe",
  "version": 1,
  "name": "滑动两次",
  "description": "在当前页面先等待，再上滑。",
  "devices": [],
  "entry": "swipe",
  "model_limit": 10,
  "nodes": [
    {"id": "swipe", "label": "每隔 25 秒上滑，共两次", "type": "swipe_loop", "interval": 25, "count": 2, "minutes": null, "long": false, "next": "end"},
    {"id": "end", "label": "完成", "type": "end", "result": "succeeded"}
  ],
  "tests": [{"node": "swipe", "text": "interval=25,count=2", "expected": "valid"}]
}
```

`devices` 为空表示通用工作流；专用界面应填写已适配手机编号。版本不可覆盖；改动需增加 `version`。`active` 和 `sample_validated` 由服务管理，提交值不会用于绕过验证。

步骤说明与编号会直接显示在网页，使用用户能读懂的中文说明。

| 步骤类型 | 参数 | 行为 |
| --- | --- | --- |
| `wait` | `seconds` | 等待，最大 86400 秒 |
| `launch` | `package` | 打开应用，检查前台应用 |
| `tap_text` | `text`, `verify` | 控件文字优先，文字识别补充；匹配唯一位置；检查结果文字 |
| `tap_point` | `x`, `y`, `guard`, `verify` | 相对坐标；点按前检查页面文字，之后检查结果文字 |
| `swipe` | `from`, `to`, `duration_ms` | 单次相对坐标滑动；异常恢复时不重放未确认动作 |
| `key` | `key`, `verify` | BACK、HOME、ENTER；检查结果文字 |
| `branch` | `cases`, `otherwise`, 可选 `guard` | `cases` 为 `contains` 与 `next` 数组；页面依据 `guard` 匹配时，未命中条件直接走 `otherwise`，无需模型；未知画面再调用模型 |
| `swipe_loop` | `interval`, `count`, `minutes`, `long` | 当前页面定时上滑；次数或时长至少设置一个 |
| `qishui_navigate` | 无 | 已采样的汽水音乐金币入口、遮挡关闭和任务入口识别 |
| `qishui_loop` | `coin_region`, `identity_region`, `warmup`, `stall`, `finish_stall`, `finish_swipes` | 区域均为 `[左,上,右,下]` 相对坐标；多帧金币识别和视频切换复查 |
| `end` | `result` | `succeeded` 或 `needs_help` |

除 `branch` 与 `end`，每个步骤需要 `next`。分支和 `next` 必须引用存在的步骤；入口需要能够到达结束步骤。

模型只选择声明过的分支或导航目标。API 不接收任意脚本、电脑 Shell 或任意 ADB 命令。

## 验证与发布

`tests` 是文本样本：`node` 指定判断步骤，`text` 是识别结果，`expected` 是预期分支。分支步骤须提供样本；`guard` 未匹配的样本预期为 `unknown`，已知页面的否定分支需提供 `guard`；汽水任务须覆盖视频与网络阻塞样本。文字样本检查、运行引擎的图像规则检查、真机试跑分别验证不同环节。

试跑接口会自动校验文本样本。成功试跑后自动启用；限定多台手机的工作流需要各台都通过真机试跑。手动发布与回退也须具备该版本的样本通过和成功试跑记录。

```sh
python3 scripts/studio.py request POST /workflows/编号@版本/publish
```

恢复任务时，重新识别当前页面。一般点击、按键若无法确认上次结果则停止；定时滑动不重复发送待确认动作，并检查原来的应用是否仍在前台；汽水任务重新观察金币和视频状态。

## HTTP 接口

所有接口前缀 `/api`；Agent 使用 `Authorization: Bearer <令牌>`。响应错误提供 `error` 字段。

| 方法与路径 | 用途 |
| --- | --- |
| GET `/devices` | 设备、连接方式和常亮状态 |
| POST `/devices/refresh` | 重新发现设备 |
| PUT `/devices/{id}` | `name` 或 `keep_awake` |
| GET `/devices/{id}/screenshot` | 当前截图 PNG |
| GET / POST `/workflows` | 查询版本 / 提交新版 |
| POST `/workflows/{id}@{version}/validate` | 校验与文本样本验证 |
| POST `/workflows/{id}@{version}/publish` | 启用或回退版本 |
| GET / POST `/runs` | 查询 / 运行，参数见下方 |
| DELETE `/runs/{id}` | 清理已结束的记录，归档与截图保留；进行中的任务拒绝清理 |
| POST `/runs/{id}/stop` | 停止，包括等待中的任务 |
| GET `/runs/{id}/events` | 判断与行动记录 |
| GET `/runs/{id}/export` | 工作流快照、结果、记录和截图路径 |
| GET / POST `/schedules` | 查询 / 创建定时 |
| PUT / DELETE `/schedules/{id}` | 更新 / 删除定时 |
| GET `/settings` | 后台与模型设置 |
| PUT `/settings/model` | `endpoint`, `model`, `api`, `key_env` |
| GET `/shots/{run}/{file}` | 关键截图 PNG |
| GET `/ws` | 设备与运行变更事件；事件到达后重新查询 |

运行参数：`device_id`, `workflow_id`, `trial`；试跑额外指定 `version`。普通运行使用当前启用版本。

定时参数：`name`, `device_id`, `workflow_id`, `enabled`, `rule`。

| rule | 格式 |
| --- | --- |
| 单次 | `{"kind":"once","at":"2026-10-14T10:00:00+08:00"}` |
| 每天 | `{"kind":"daily","time":"09:00"}` |
| 每周 | `{"kind":"weekly","time":"09:00","days":[1,3,5]}`，周一为 1 |
| 间隔 | `{"kind":"interval","minutes":60}` |

定时绑定工作流编号，下次执行读取已启用版本；运行中的任务版本固定。离线、忙碌、后台停机错过的定时点都跳过，不积压。间隔规则以保存时间为起点。

## 滑动运行设置

网页「自动化任务 → 定时滑动」可输入总时长（分钟）和滑动间隔（秒），默认 15 分钟、25 秒。先等待再滑动，到达时长结束；时长小于间隔时只等待，不滑动。网页记住上次运行设置。

「定时任务」中的同名输入随每条规则保存，修改时自动带回。执行中的任务使用启动时的设置，修改不会影响已开始的任务。

`POST /api/runs` 和定时创建／修改接口接受 `"swipe": {"minutes":15,"interval":25}`。总时长大于 0、最多 10080 分钟，间隔大于 0、最多 86400 秒；支持小数。该参数覆盖运行快照中的次数限制，不改变工作流原版本。省略时沿用原定义。

手机截图兼容多显示屏设备：ADB 输出中的显示屏提示会被移除，图片校验通过后才交给网页和识别流程。截图损坏时返回明确错误，可重新截图。

折叠屏截图每次重新检查内外屏状态，指定当前亮着的物理屏幕；无法确定目标时提示重新点亮屏幕，避免误截黑着的内屏。

## 截图分段计时

截图接口仍返回 PNG。USB 优先取带 sRGB 色彩空间信息的 RGBA 或 RGBX 原始画面，由电脑无损生成 PNG；格式不支持或数据损坏时回退手机 PNG。无线连接直接使用手机 PNG。识别流程复用已解析图片，每次请求重新取图。

响应增加 `Cache-Control: no-store` 与 `Server-Timing`。计时单位为毫秒：`device` 为设备核验，`display` 为亮屏检查，`capture` 为手机取图与传输，`decode` 为画面解析，`encode` 为电脑生成 PNG，`total` 为服务端总耗时。`source` 的 `raw` 表示电脑生成 PNG，`png` 表示手机生成 PNG；发生回退时，计时包含失败尝试。

```sh
python3 scripts/studio.py request GET /devices/手机自身编号/screenshot --output /绝对路径/当前画面.png --timings
```

同次截图使用已核验的 ADB 连接编号，断线后报错；旧版 ADB 无连接编号时保留逐步核验。该优化不改变运行互斥、动作恢复或截图标注规则。真机结果见 [截图提速与验收](screenshot-performance.md)。

## 招财鸭点击喂鸭

`feed-duck` 第 2 版在折叠屏外屏固定点击 `(528,1866)`。不识别、不截图、不校验页面。请先打开招财鸭并保持外屏，执行期间不要换页面。

设置点击次数、间隔秒数；立即点击第一次，其后按间隔执行。支持 `"tap":{"count":10,"interval":5}`，定时规则保存各自设置。点击次数 1 到 10000，间隔 1 到 86400 秒。第 1 版保留供回退。

断线等待、停止和同机互斥仍有效；未确认成功的点击不会重放。

## 记录清理

网页「执行记录」默认显示最新一条详情。详情中的“清理这条记录”处理单条，顶部“清理已结束”处理点击时已结束的记录；两种操作均需确认。已完成、已停止和需要处理的记录可清理，执行中、等待手机和恢复中的任务会保留。

批量清理逐条调用 `DELETE /api/runs/{id}`。清理后从列表移除，原始运行、判断记录和截图保留在本机；该接口不删除定时规则或改变工作流启用状态。部分记录清理失败时，网页显示成功数量与失败原因，可重新清理剩余记录。

## 截图标注与定点点击

在「设备管理 → 查看设备详情」点“标注点击位置”，或在「自动化任务 → 定点点击」选择手机后标注。弹窗显示当前亮着的屏幕截图，点击目标后出现红点，按“确定位置”保存。标注本身不会操作手机；取消不改变已保存位置。

截图缩放后的点选位置换算为原图像素；各台手机单独保存坐标和原图尺寸，重启保留。定点点击开始时将坐标写入运行快照，之后不截图、不识别。换屏、旋转或页面布局变化后应重新标注。

首次点“试跑此版本”；通过后启用，随后可直接运行或设置定时。定时触发使用该手机当时保存的位置。设备修改接口接受 `"tap_point":{"x":528,"y":1866,"width":1060,"height":2376}`，坐标必须在原图范围内。

## 唤醒与锁屏

设备详情提供“唤醒／尝试解锁”和“锁屏”。唤醒使用 Android 唤醒按键及 `wm dismiss-keyguard`，仅适用于无密码或系统允许 ADB 解除的锁屏；需要密码时提示用户自行解锁。不在已打开的页面上滑动。锁屏使用睡眠按键，重复点击不会反向唤醒。运行任务期间先停止任务，再操作屏幕。

`POST /api/devices/{id}/screen` 接受 `{"action":"wake"}` 或 `{"action":"lock"}`，返回操作结果 `message`。
