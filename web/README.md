# NarraState Web

Vue 3、Vite、TypeScript strict、Pinia 和 Vue Router 构成的本地 Web 客户端。它只调用 `/api/v1`，不保存 API Key，也不计算权威叙事状态。

开发时先在仓库根目录启动 Rust API，然后运行：

```bash
npm ci
npm run dev
```

Vite 将 `/api` 代理到 `http://127.0.0.1:3000`。发布构建使用 `npm run build`，输出的 `dist/` 由 Axum 或 Docker 镜像托管。

质量命令：

```bash
npm run typecheck
npm test -- --run
npm run build
```

普通玩家界面不得渲染 phase、stress、defense budget、Prompt、token、隐藏事实或未解锁 disclosure。开发者抽屉必须先显示明确剧透警告。
