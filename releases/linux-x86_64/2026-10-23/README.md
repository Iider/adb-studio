# Linux 原生部署包

本目录保存 2026-10-23 已部署到飞牛并验证的版本，对应源码提交 `05e3e80`。无需 Docker。

| 文件 | 用途 |
| --- | --- |
| `adb-studio-linux-x86_64.tar.gz` | 后台程序、网页、投屏组件、安装脚本及说明 |
| `adb-studio-linux-x86_64.tar.gz.sha256` | 检查下载或复制是否完整 |

## 适用设备

- Linux x86_64；`uname -m` 应显示 `x86_64`。此包不适用于 ARM 或 macOS。
- 后台为静态 musl 程序，不要求 glibc 2.36；实测系统为飞牛 Debian 12 x86_64。包内通用部署文档中的 glibc 要求适用于常规 CI 构建，不适用于本目录这份静态包。
- 后台安装需要 Python 3.10+、systemd 用户会话；ADB 需另外安装为适合本机的 Linux 版本。
- 不包含个人数据库、截图、模型密钥、访问凭据或 ADB 配对信息；新安装需自行配置。

## 解压、试用

在 Linux 上进入本目录，或将压缩包和校验文件复制到同一个目录后执行：

```sh
sha256sum -c adb-studio-linux-x86_64.tar.gz.sha256
tar -xzf adb-studio-linux-x86_64.tar.gz
cd adb-studio-linux-x86_64
adb devices -l
./adb-studio
```

在手机上允许 USB 调试；USB 权限不足时按实际手机编号配置设备权限。浏览器打开 `http://127.0.0.1:17841`；按 Ctrl+C 结束前台试用。模型识别功能需按包内 `docs/vision-recognition.md` 配置模型和密钥。

远程 Linux 的 `127.0.0.1` 是它自己。可在自己的电脑上执行下面的转发，再打开 `http://127.0.0.1:17851`：

```sh
ssh -NT -L 127.0.0.1:17851:127.0.0.1:17841 用户名@Linux地址
```

直接通过局域网投屏需配置 HTTPS 并信任证书，见包内 `docs/feiniu-native.md`；普通局域网 HTTP 通常无法使用浏览器投屏。

## 安装为后台服务

先结束前台试用，在解压目录执行：

```sh
python3 scripts/install-service.py install --adb /实际路径/platform-tools/adb
python3 scripts/install-service.py status
```

安装位置默认是 `~/.local/share/adb-studio/`；安装脚本会配置用户服务与自启。需要退出 SSH 后仍运行时，由管理员为该用户开启 Linger：

```sh
sudo loginctl enable-linger "$USER"
```

模型密钥可放在安装目录的 `.env`，文件权限设为 `600`；修改后重启工作台。不要将密钥提交到 Git。

## 更新已有安装

1. 先妥善停止正在运行或暂停中的任务。
2. 停止后台，备份完整安装目录及 `~/.android/`；停服务后复制数据库，避免遗漏正在写入的数据。
3. 在新包的解压目录运行同一条安装命令；它保留已有数据库、模型密钥与 systemd 覆盖配置。
4. 检查手机连接、页面、模型设置和投屏开关。

```sh
systemctl --user stop adb-studio.service
# 此时备份 ~/.local/share/adb-studio/ 和 ~/.android/
python3 scripts/install-service.py install --adb /实际路径/platform-tools/adb
systemctl --user status adb-studio.service --no-pager
```

当前飞牛使用独立 ADB 用户服务与 5038 端口，已保存在原服务覆盖配置中；更新不要删除覆盖配置，也不要启动另一套 ADB 来抢占手机。

停止及移除自启：

```sh
python3 scripts/install-service.py remove
```

此命令保留运行数据。回退时先停服务，再恢复升级前备份及旧程序。

## 已验证范围

飞牛执行 24 项单元检查、模拟设备集成、纯视觉隔离、投屏控制、安装路径及发行包检查，均通过。iQOO 真机通过 HTTPS 和 SSH 转发投屏，实体屏幕可关闭、点亮，息屏时仍保持操作状态。未验证其他架构、其他 Linux 发行版及整台 NAS 重启。
