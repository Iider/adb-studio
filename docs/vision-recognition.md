# 模型看图检查

“系统设置 → 识别检查”可读取手机当前画面或上传本地 PNG，返回判断、位置或整数。检查不会点击手机。截图会发送到已保存的模型服务。

工作流只通过画布编辑器创建。旧纯视觉任务与其专用动作已移除；独立看图检查继续保留。画布“页面文字”条件优先使用已配置模型，否则使用本地读字；模型优先已通过用户验收并部署。直接填写 Key／Base URL 仍待实施，当前规则见[整改方案](recognition-fallback-plan.md)。

## 配置

系统设置当前填写完整接口地址、模型名称、Responses／Chat 格式和密钥变量名称。密钥变量默认 MODEL_KEY，真实 Key 放入后台环境变量或 .env；不能将真实 Key 填入“密钥变量”。地址原样请求，不自动补接口路径。保存只检查字段，不代表看图成功。

## 只读检查接口

`POST /api/recognition/check` 使用访问凭据与同源检查。device_id 与 image_base64（纯 PNG 的 Base64）只能选一个：

```json
{"device_id":"手机自身编号","query":{"condition":"指定页面","target":"目标按钮","region":[0,0,1,1],"read_number":false}}
```

target 可省略。region 为宽高比例 `[左,上,右,下]`，默认全屏。返回坐标换算为原图像素；整数放在 result.coin 字段，可读取一般整数。

返回 id、image、captured_at、size、query、model、calls、capture_ms、model_ms、result、error。无法确认返回 unknown；无效返回与越界显示错误。参数错误返回 400，模型失败仍保存检查记录。`GET /api/recognition/samples/{UUID.png}` 读取样本，需要相同凭据。

PNG 最多 8 MiB，宽高各最多 8192；一次检查最多请求 2 次，每次最多等待 60 秒，同时仅接受一项检查。样本和记录保存在本地 data 中，不包含密钥，暂不自动清理。

## 检查

```sh
cargo test
cargo build
python scripts/check-vision.py
```

模拟检查覆盖两类接口、来源限制、坐标与无效结果，不调用真实模型、不操作真机。真实识别准确性和等待时间需用实际截图验收。
