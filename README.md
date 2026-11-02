# Hawwwker 货客 · v26.03

AI 多平台购物搜索、比价与盯价桌面应用（Tauri 2 + Vue 3 + TypeScript）。
完整设计见 `docs/开发手册-v26.03.md`；本版改动见 `docs/releases/v26.03/RELEASE_NOTES.md`。

## 快速开始（Windows）

前置：Node.js 20+、Rust（rustup，stable）、Visual Studio C++ 生成工具（"使用 C++ 的桌面开发"）、Edge 或 Chrome。

```bat
cd app
scripts\setup.bat      :: 安装依赖并检查环境（首次）
scripts\dev.bat        :: 桌面开发模式（热更新）
scripts\build.bat      :: 打包：生成 exe 与安装包
```

打包产物：

- 免安装程序：`src-tauri\target\release\hawwwker.exe`
- 安装包：`src-tauri\target\release\bundle\nsis\Hawwwker_26.3.0_x64-setup.exe`（以及 `msi\`）

只想看界面：`npm install && npm run dev`，用浏览器打开 http://localhost:1420（自动使用模拟数据；`?onboarding=1` 预览引导，`?noai=1` 预览 AI 未连接时的拦截，`?ollama=1` 模拟检测到本机模型）。`npm run build:web` 生成可双击打开的单文件网页预览版（`dist-web/index.html`）。

## 发布

```bash
# Linux 交叉编译 Windows（需要 mingw-w64、makensis、x86_64-pc-windows-gnu 标准库）
scripts/release.sh 26.03 <证据目录>
```

输出到 `../releases/v<版本>/`，命名固定为：`README.md`、`RELEASE_NOTES.md`、`SHA256SUMS.txt`、`THIRD_PARTY_NOTICES.md`、`Hawwwker-v<版本>-release-evidence.zip`、`-source.zip`、`-web.zip`、`-windows-x64-portable.exe`、`-windows-x64-setup.exe`。说明文档取自 `docs/releases/v<版本>/`，便携版启动器脚本在 `src-tauri/portable/portable.nsi`。

## 首次使用

1. 引导：选择角色（消费者 / 商户 / 平台分析）。
2. 连接 AI（必需）：选服务商、填 API Key、测试通过后继续。推荐 DeepSeek；检测到本机 Ollama / LM Studio 时可一键使用（免费）。
3. 选择平台：按所在地区预选并标出是否需要登录（国内版 / 国际版可同时启用），之后可在「平台 · 平台目录」调整，并用"检测"按钮验证站点是否可用。
4. 需要登录的平台到「平台 · 账号与登录」点「登录」，在弹出的官网窗口里登录一次即可长期复用。

## 目录

```
app/
├── src/                 Vue 3 前端（views / components / stores / api）
│   └── api/             types.ts（IPC 契约）· index.ts（命令封装）· events.ts · mock.ts（浏览器预览数据）
├── src-tauri/           Rust 内核
│   ├── src/ai/          模型客户端、规划、校验、总结、Copilot、预设
│   ├── src/browser/     CDP 连接（管道 / 端口）、进程启动（Job Object / PDEATHSIG）、浏览器管理、搜索页抽取、登录状态机
│   ├── src/search/      规则预解析、卡片解析、过滤聚类、搜索管线
│   ├── src/*.rs         平台注册表、追踪、监控、调度、提醒、汇率、洞察、价格判断(deal)、平台自检(selftest)、命令
│   ├── portable/        便携版启动器（NSIS）
│   └── tests/           浏览器端到端测试（需要本机 Chromium）
├── docs/releases/       各版本 README 与发布说明
└── scripts/             Windows 脚本、release.sh、third_party_notices.py
```

## 测试

```bat
npm run typecheck
cd src-tauri && cargo test
:: 端到端（用本机 Chrome/Edge 跑模拟商城 + 模拟大模型）
set HAWWWKER_BROWSER=C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe
set HAWWWKER_HEADLESS=1
cargo test --test browser_e2e --test pipeline_e2e -- --ignored --nocapture
```

## 数据位置

安装版 `%APPDATA%\com.hawwwker.app\`（便携版在 exe 旁边的 `Hawwwker-data\`，也可用环境变量 `HAWWWKER_DATA_DIR` 指定）：`hawwwker.db`（SQLite）、`browser-profile\`（自动化浏览器的登录会话，只在本机）、`exports\`、日志在 `%LOCALAPPDATA%\com.hawwwker.app\logs\`。
