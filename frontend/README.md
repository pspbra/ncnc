# NCNC WebUI

NCNC WebUI 是 NCNC 媒体管理服务的浏览器管理界面，基于 Vue 3、TypeScript 和 Vite 构建，适配桌面与移动端。

## 主要功能

- 浏览媒体库、季集状态和媒体详情
- 查看播出日历
- 搜索并订阅剧集或添加电影
- 管理下载与上传任务
- 查看服务日志和系统状态
- 配置媒体目录、外部服务和自动化任务
- 支持明暗模式与多套主题

## 运行要求

- Linux
- Node.js 20 或更高版本
- npm
- 可访问的 NCNC Backend

不要在不同操作系统之间复用 `node_modules`。切换环境后请重新安装依赖。

## 本地开发

```bash
cd frontend
npm ci
npm run dev
```

开发服务器默认运行在 `http://localhost:5173/webui/`。

当前开发环境的 API 地址为 `http://localhost:8000`，定义在 `src/api/modifiedAxios.ts`。由于 NCNC Backend 当前监听 `3000` 端口，联调时需要通过反向代理将 `8000` 转发到后端，或者按本地环境调整该地址。生产构建使用同源 API。

## 编译

```bash
npm run build
```

构建过程会执行 Vue/TypeScript 类型检查，并将生产文件输出到 `dist/`。应用的公共路径为 `/webui/`。

仅生成静态文件时可以运行：

```bash
npm run build-only
```

## 部署

推荐将 `dist/` 中的文件复制到 NCNC Backend 工作目录的 `webui/`：

```bash
mkdir -p ../backend/webui
cp -r dist/. ../backend/webui/
```

启动后端后，通过 `http://<服务器地址>:3000/webui` 访问。也可以使用 Nginx 托管静态文件，但需要保证：

- 页面和静态资源位于 `/webui/`
- `/v1/` 请求与前端保持同源并转发到 NCNC Backend
- SPA 路由回退到 `index.html`

## 常用命令

```bash
npm run dev          # 启动开发服务器
npm run build        # 类型检查并构建
npm run build-only   # 仅构建静态文件
npm run preview      # 预览生产构建
npm run type-check   # Vue/TypeScript 类型检查
npm run test:unit    # Vitest 单元测试
npm run test:e2e     # Playwright 端到端测试
```

详细的目录结构、页面说明、API 封装和开发约定请参阅 [开发说明.md](开发说明.md)。

## 技术栈

Vue 3 · TypeScript · Vite · Pinia · Vue Router · Naive UI · Axios

## 许可证

项目许可证见仓库根目录的 [LICENSE](../LICENSE)。
