# 飞牛原生部署

## 访问

| 入口 | 地址 | 用途 |
| --- | --- | --- |
| 局域网 HTTPS | https://192.168.2.25:17842/ | 信任证书后，直接管理与投屏 |
| 局域网 HTTP | http://192.168.2.25:17841/ | 设备、任务、识别检查等管理功能 |
| 当前 Mac | http://127.0.0.1:17851/ | 同一套飞牛工作台，支持浏览器投屏 |

推荐 HTTPS 入口，客户端首次使用需要导入并信任证书。普通局域网 HTTP 通常不能投屏。

Mac 的 `17851` 入口通过 SSH 转发到同一套飞牛服务，作为备用入口；它需要当前 Mac 在线并已登录。Mac 自己的 `17841` 工作台使用独立数据。其他电脑也可建立以下本机转发：

```sh
# 在其他电脑执行；需先配置该电脑到飞牛的 SSH 登录
ssh -NT -L 127.0.0.1:17851:127.0.0.1:17841 weio@192.168.2.25
```

## HTTPS 与证书

飞牛使用独立用户服务 `adb-studio-https.service`，将 `192.168.2.25:17842` 的 HTTPS 转发至本机工作台 `127.0.0.1:17841`。复用 Debian 官方 nginx 1.22.1-9+deb12u10 的可执行文件，单独解压到用户目录；不更改飞牛系统 nginx 的程序与配置。支持 TLS 1.2／1.3、原始 Host／Origin 与 WebSocket 转发。

| 内容 | 位置 |
| --- | --- |
| 服务配置 | `/home/weio/.config/systemd/user/adb-studio-https.service` |
| HTTPS 配置 | `/home/weio/.local/opt/adb-studio/https/nginx.conf` |
| 独立 nginx | `/home/weio/.local/opt/adb-studio/https/vendor/usr/sbin/nginx` |
| 服务日志 | `/home/weio/.local/state/adb-studio/https.log` |
| 代理错误日志 | `/home/weio/.local/opt/adb-studio/https/logs/error.log` |
| 证书 | `/home/weio/.local/share/adb-studio/tls/studio.crt` |
| 私钥 | `/home/weio/.local/share/adb-studio/tls/studio.key`（权限 `600`，只留在飞牛） |
| Mac 证书副本 | `~/Library/Application Support/ADB Studio/Certificates/feiniu-studio.crt` |

证书仅包含 IP `192.168.2.25`，有效期至北京时间 **2027-10-22 09:22:15**。更换飞牛 IP 或证书到期时，需要重新签发并让客户端重新信任。自签名证书不会自动成为系统信任的证书；浏览器中的“继续访问”不等同于完成系统信任。

SHA-256 指纹：`31:D6:D5:25:8C:C6:5B:11:CE:31:D9:E4:87:19:86:2F:61:68:3D:E8:3A:3C:A5:0C:BC:BC:76:25:F6:B3:6E:4F`。

当前 Mac 已将这张服务证书导入登录钥匙串，并信任它的 SSL 用途；访问地址由证书里的 IP 校验。重新导入时使用以下命令，若系统要求授权，由本人完成验证：

```sh
security add-trusted-cert -r trustRoot -p ssl \
  -k "$HOME/Library/Keychains/login.keychain-db" \
  "$HOME/Library/Application Support/ADB Studio/Certificates/feiniu-studio.crt"
```

其他电脑需要单独导入这张公开证书；不要分发 `studio.key` 私钥。导入后如果浏览器仍显示旧的证书错误，关闭并重新打开浏览器再访问。证书信任验证：

```sh
security verify-cert -c "$HOME/Library/Application Support/ADB Studio/Certificates/feiniu-studio.crt" -p ssl -n 192.168.2.25 -L
```

只停止 HTTPS：

```sh
ssh feiniu 'systemctl --user disable --now adb-studio-https.service'
```

这保留 HTTP 与 SSH 转发入口。HTTPS 加密连接，工作台仍适用于可信家庭网络；证书不提供登录保护。

## 安装位置

2026-10-22 实测：Debian 12、x86_64、glibc 2.36，使用 `weio` 用户运行。用户已开启 `Linger`，退出 SSH 后服务仍运行，并随系统启动。

| 内容 | 飞牛路径 |
| --- | --- |
| 程序、网页、模型配置 | `/home/weio/.local/share/adb-studio/` |
| 数据库、截图、访问凭据 | `/home/weio/.local/share/adb-studio/data/` |
| 发行包与安装脚本 | `/home/weio/.local/opt/adb-studio/releases/2026-10-22/adb-studio-linux-x86_64/` |
| ADB 37.0.1 | `/home/weio/.local/opt/android/platform-tools/adb` |
| 工作台日志 | `/home/weio/.local/state/adb-studio/service.log` |
| ADB 日志 | `/home/weio/.local/state/adb-studio/adb.log` |
| 用户服务 | `/home/weio/.config/systemd/user/adb-studio.service` |
| 服务覆盖配置 | `/home/weio/.config/systemd/user/adb-studio.service.d/native.conf` |
| ADB 用户服务 | `/home/weio/.config/systemd/user/adb-studio-adb.service` |

ADB 官方下载地址：`https://dl.google.com/android/repository/platform-tools-latest-linux.zip`。本次文件 SHA-256 为 `d230f13842f60f782a8645f9c813f8f845bf36089ea7289f28c48f17979313f1`；该网址会更新版本，后续下载需重新核对版本与校验值。

飞牛模型配置与 Mac 相同，密钥保存在飞牛安装目录 `.env`，权限为 `600`。访问凭据独立生成；Mac 的工作流、执行记录、计划任务没有迁入。Linux 使用视觉模型识别，未安装苹果本地文字识别组件。

## 手机与 USB 权限

已登记：iQOO、V2171A、`3430131496001EB`。手机已允许飞牛的 USB 调试。

`/etc/udev/rules.d/51-adb-studio.rules` 仅为该手机授予 `weio` 读写权限，插拔后自动应用：

```text
SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{serial}=="3430131496001EB", OWNER="weio", MODE="0600"
```

新增 USB 手机需为实际编号添加规则、重新加载规则，并在手机上允许 USB 调试。不要给所有 USB 设备统一开放权限。

飞牛 ADB 独立使用本机 `5038` 端口，工作台依赖 `adb-studio-adb.service` 启动。ADB 服务命令为：

```text
/home/weio/.local/opt/android/platform-tools/adb -L tcp:5038 nodaemon server
```

服务端 `-L` 使用不带主机名的地址，默认仅本机监听；官方 ADB 37 不支持在这里填写 `tcp:127.0.0.1:5038`。客户端按以下命令指定服务：

```sh
ssh feiniu 'ADB_SERVER_SOCKET=tcp:127.0.0.1:5038 ~/.local/opt/android/platform-tools/adb devices -l'
```

## 日常维护

在 Mac 执行：

```sh
ssh feiniu 'systemctl --user status adb-studio.service adb-studio-adb.service adb-studio-https.service --no-pager'
ssh feiniu 'systemctl --user restart adb-studio.service'
ssh feiniu 'tail -n 50 ~/.local/state/adb-studio/service.log'
```

当前飞牛用户不能读取系统日志，因此两个服务写入上表中的日志文件。服务设置了失败重启；ADB 服务只监听 `127.0.0.1:5038`，工作台监听 `0.0.0.0:17841`，允许网址为 `http://192.168.2.25:17841`、`https://192.168.2.25:17842` 及本机 HTTP。

更新前先妥善停止任务，并备份完整安装目录及 `~/.android/` 配对信息。替换新发行包后，在飞牛的解压目录执行：

```sh
python3 scripts/install-service.py install --adb /home/weio/.local/opt/android/platform-tools/adb
```

安装脚本保留数据库、密钥及服务覆盖配置。备份正在使用的数据库需使用 SQLite 备份接口，或先停止工作台再复制。

停止工作台：

```sh
ssh feiniu 'systemctl --user disable --now adb-studio-https.service adb-studio.service'
ssh feiniu 'systemctl --user stop adb-studio-adb.service'
```

Mac 转发自启配置位于 `~/Library/LaunchAgents/local.adb-studio-feiniu-tunnel.plist`，日志位于 `~/Library/Logs/ADB Studio/feiniu-tunnel.log`。关闭转发：

```sh
launchctl bootout gui/$(id -u)/local.adb-studio-feiniu-tunnel
```

关闭转发不停止飞牛服务；要取消 Mac 下次登录时自启，还需移走上述转发配置文件。

## 实测范围

- 原生服务启动、退出 SSH 后持续运行、重复安装与重启后保留配置。
- 局域网页面、模型设置、独立访问凭据、拒绝非允许来源。
- HTTPS 页面、Secure Cookie、错误来源拒绝、真实 WSS 投屏流通过。
- Mac 系统证书验证通过；使用系统信任访问 HTTPS 返回 200；内置浏览器已打开 HTTPS 工作台。
- 飞牛调用视觉模型识别已有截图成功，一次模型调用。
- iQOO USB 连接、型号核对、真实截图；单次截图约 1.4 秒。
- 经 Mac 转发收到 H.264 投屏流，576×1280；首次收到约 1.2 秒。

浏览器内连续画面与手动操作仍需用户验收。没有重启整台飞牛验证开机过程；已核对服务启用状态与 `Linger=yes`。Mac 与飞牛操作同一手机前，先停止另一端的任务，避免两端同时控制。
