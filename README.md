# Hawwwker 货客 · v26.01

AI 多平台购物搜索、比价与盯价桌面应用（Tauri 2 + Vue 3 + TypeScript）。
完整设计见上一级目录的《开发手册-v26.01.md》。

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
- 安装包：`src-tauri\target\release\bundle\nsis\Hawwwker_26.1.0_x64-setup.exe`（以及 `msi\`）

只想看界面：`npm install && npm run dev`，用浏览器打开 http://localhost:1420（自动使用模拟数据；`?onboarding=1` 预览引导，`?noai=1` 预览 AI 未连接时的拦截）。

## 首次使用

1. 引导：选择角色（消费者 / 商户 / 平台分析）。
2. 连接 AI（必需）：选服务商、填 API Key、测试通过后继续。推荐 DeepSeek；本机 Ollama 也可。
3. 选择地区与平台（国内版 / 国际版可同时启用），之后可在「平台 · 平台目录」调整。
4. 需要登录的平台到「平台 · 账号与登录」点「登录」，在弹出的官网窗口里登录一次即可长期复用。

## 目录

```
app/
├── src/                 Vue 3 前端（views / components / stores / api）
│   └── api/             types.ts（IPC 契约）· index.ts（命令封装）· events.ts · mock.ts（浏览器预览数据）
├── src-tauri/           Rust 内核
│   ├── src/ai/          模型客户端、规划、校验、总结、Copilot、预设
│   ├── src/browser/     CDP 连接、浏览器管理、搜索页抽取、登录状态机
│   ├── src/search/      规则预解析、卡片解析、过滤聚类、搜索管线
│   ├── src/*.rs         平台注册表、追踪、监控、调度、提醒、汇率、洞察、命令
│   └── tests/           浏览器端到端测试（需要本机 Chromium）
└── scripts/             Windows 脚本
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

`%APPDATA%\com.hawwwker.app\`：`hawwwker.db`（SQLite）、`browser-profile\`（自动化浏览器的登录会话，只在本机）、`exports\`、日志在 `%LOCALAPPDATA%\com.hawwwker.app\logs\`。
