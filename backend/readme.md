# NCNC Backend

NCNC 的媒体订阅、下载与整理后端，使用 Rust、Actix Web 和 Tokio 构建。它负责聚合媒体元数据、查找资源、调度下载、处理媒体文件，并为 WebUI 提供 API 与静态文件服务。

## 主要功能

- 通过 TMDB、TVDB 和 SkyHook 获取媒体、季集及播出信息
- 通过 Jackett 搜索资源，并交由 aria2 下载
- 自动补集与 RSS 自动下载
- 使用 FFprobe/FFmpeg 检测、转码、提取字幕和整理文件
- 将整理完成的媒体同步到 OpenList
- 提供媒体库、任务、日志、设置及系统状态 API
- 托管 NCNC WebUI，提供统一的管理入口

## 运行要求

- Linux
- Rust stable 与 Cargo
- `/usr/bin/aria2c` 以及工作目录中的 `aria2.conf`
- FFmpeg、FFprobe；启用自动压缩时需要可用的 Intel QSV 环境
- 可选外部服务：TMDB、TVDB、Jackett、OpenList
- 媒体库和处理目录需要足够空间与读写权限，且应位于同一文件系统

## 编译

```bash
cd backend
cargo build --release --locked
```

二进制文件生成在 `target/release/ncnc`。也可以使用发布脚本生成精简文件：

```bash
chmod +x build-release.sh
./build-release.sh
```

发布文件会写入 `dist/ncnc`，脚本结束时会清理 Cargo 构建目录。

## 运行

开发模式：

```bash
cd backend
RUST_LOG=info cargo run --locked
```

发布模式：

```bash
cd backend
RUST_LOG=info ./dist/ncnc
```

服务当前固定监听 `0.0.0.0:3000`：

- 管理界面：`http://localhost:3000/webui`
- API 前缀：`http://localhost:3000/v1`

首次启动会在当前工作目录生成 `config.json` 和运行数据。默认账号为 `admin`，默认密码为 `12345`，登录后请立即修改。

> 自动下载、RSS 和 OpenList 自动上传在默认配置中处于开启状态。首次部署时建议先停止服务，检查 `config.json` 中的目录、凭据和自动化开关，再重新启动。

## 部署 WebUI

前端构建产物需要放在后端工作目录的 `webui/` 下：

```bash
cd frontend
npm ci
npm run build

mkdir -p ../backend/webui
cp -r dist/. ../backend/webui/
```

随后从 `backend` 工作目录启动服务，即可通过 `/webui` 访问管理界面。

## 配置说明

主要配置保存在运行目录的 `config.json`，包括：

- 媒体库与临时处理路径
- TMDB、TVDB、Jackett、aria2 和 OpenList 连接信息
- 代理、自动下载、RSS、自动上传与媒体压缩开关
- 登录账号、密码哈希与 JWT 密钥

`config.json`、`data.json` 等运行数据不会提交到 Git。完整配置字段、API、处理流程和故障排查请参阅 [开发说明.md](开发说明.md)。

## 技术栈

Rust 2021 · Actix Web · Tokio · Serde · Reqwest

## 许可证

项目许可证见仓库根目录的 [LICENSE](../LICENSE)。
