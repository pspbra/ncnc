# NCNC

NCNC 是一个自托管的媒体订阅、下载与整理服务，主要面向电视剧／动漫，也支持电影搜索与手动上传。

- **后端** `backend/`：Rust + Actix Web + Tokio，负责元数据聚合、资源搜索、下载调度、转码整理与云端同步，同时托管 WebUI。
- **前端** `frontend/`：Vue 3 + TypeScript + Vite 管理界面，构建后以静态文件形式由后端提供。

后端源码说明见 [backend/README.md](backend/README.md)，前端见 [frontend/README.md](frontend/README.md)。

## 运行环境

只支持 Linux（x86_64）。发布二进制为 **musl + rustls 静态链接**（static-pie），不依赖 glibc、OpenSSL 等动态库，可在任意 x86-64 Linux 上直接运行。HTTPS 根证书使用 rustls 内置的 webpki-roots，不读取系统 CA 存储。

| 依赖 | 说明 |
| --- | --- |
| `/usr/bin/aria2c` | 下载器，启动时由后端拉起；缺失只影响下载功能 |
| `aria2.conf` | 放在工作目录下，RPC 地址／端口／secret 需与 `config.json` 一致 |
| FFmpeg / FFprobe | 仅自动压缩开启时检测；压缩使用 Intel QSV，需要可用核显 |
| TMDB / TVDB API Key、Jackett、OpenList | 均为可选外部服务，按需在 WebUI 中填写 |
| `/usr/sbin/smartctl` | 可选，用于磁盘温度 |

## 使用发布包运行

从 [Releases](../../releases) 下载 `ncnc`、`webui.zip` 和 `SHA256SUMS.txt`，校验后解压到同一个工作目录：

```bash
mkdir -p /opt/ncnc && cd /opt/ncnc
sha256sum -c SHA256SUMS.txt
unzip webui.zip          # 解压出 webui/，与 ncnc 同级
chmod +x ncnc
```

准备 `aria2.conf`，例如：

```ini
enable-rpc=true
rpc-listen-all=true
rpc-listen-port=6800
rpc-secret=替换为你自己的密钥
continue=true
```

然后启动：

```bash
cd /opt/ncnc
RUST_LOG=info ./ncnc
```

打开 `http://<服务器地址>:3000/webui`，默认账号 `admin`，密码 `12345`，登录后立即修改。

> 端口固定为 `3000`，`config.json` 里的 `server_port` 目前不参与监听。

## 首次配置要点

首次启动会在**当前工作目录**生成 `config.json`、`data.json`、`runtime_state.json`、`logs/`、`upload_cache/`、`skyhook_cache/`。所以务必固定工作目录，不要在不同目录下反复启动。

1. 在 WebUI 设置中填写媒体库路径、TMDB／TVDB／Jackett／OpenList 连接信息，以及 aria2 的 RPC 密钥。
2. `media_library_path` 与 `media_backup_path` 必须位于**同一文件系统**（用 `stat -c %d <路径>` 核对设备号），否则入库会退化为跨设备复制。
3. 下载暂存目录固定为 `/mnt/8tb/autodownload`，需要提前创建并给足权限。
4. 自动补集、RSS 自动下载与 OpenList 自动上传的默认值都是开启。初次部署建议先停服，把 `config.json` 中的 `jackett_auto_download`、`jackett_auto_rss`、`openlist_auto_upload` 改为 `false`，配置确认无误后再开启。
5. 自动压缩默认开启。环境没有 FFmpeg／QSV 时请在设置中关闭，否则无法重新封装的文件会处理失败并保留源文件。

## 从源码构建

后端：

需要一次性准备 musl 目标与链接器：

```bash
rustup target add x86_64-unknown-linux-musl
apt-get install -y musl-tools lld
cd backend
./build-release.sh          # 产物为静态链接的 dist/ncnc，脚本结束后清理该 target 的构建目录
```

脚本固定使用 `ld.lld` 链接 musl static-pie 目标；换回 binutils `ld` 会让产物启动即崩溃。

前端：

```bash
cd frontend
npm ci
npm run build
mkdir -p ../backend/webui && cp -r dist/. ../backend/webui/
```

把 `webui/` 放到后端工作目录后，从该目录启动 `dist/ncnc` 即可。

## 许可证

[GPL-2.0](LICENSE)
