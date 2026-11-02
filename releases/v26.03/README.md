# Hawwwker 货客 v26.03

AI 多平台购物搜索、比价与盯价桌面应用。你登录一次后，软件在**你自己的浏览器会话里**代你重复搜索、比价和盯价，全程可见，可以随时暂停或取消；遇到登录和验证码都由你本人完成。覆盖淘宝、京东、拼多多、闲鱼、小红书，以及 Amazon、eBay、idealo、Temu、AliExpress 等 30 多个国内外平台。

## 本目录文件

| 文件 | 说明 |
|---|---|
| `Hawwwker-v26.03-windows-x64-setup.exe` | **安装包**（推荐）。安装到当前用户，不需要管理员权限；缺少 WebView2 时会自动下载微软官方运行时 |
| `Hawwwker-v26.03-windows-x64-portable.exe` | **便携版**，免安装，双击即用。数据保存在 exe 旁边的 `Hawwwker-data` 文件夹 |
| `Hawwwker-v26.03-web.zip` | 网页预览版：解压后双击 `index.html`，用模拟数据体验全部界面（不能真实搜索） |
| `Hawwwker-v26.03-source.zip` | 完整源代码（Tauri 2 + Vue 3 + TypeScript） |
| `Hawwwker-v26.03-release-evidence.zip` | 发布证据：测试日志、联调截图、构建日志、代码审查记录 |
| `RELEASE_NOTES.md` | 本版更新内容 |
| `THIRD_PARTY_NOTICES.md` | 第三方组件与许可证 |
| `SHA256SUMS.txt` | 文件校验和 |

## 系统要求

- Windows 10 / 11 64 位（WebView2 运行时：Windows 11 自带，安装包会在缺失时自动安装）
- Microsoft Edge 或 Google Chrome（软件用它在你的会话里打开购物网站）
- 一个 AI 模型：DeepSeek（推荐 `deepseek-flash`）、通义千问、Kimi、智谱、OpenAI、Claude、Gemini 等的 API Key，或本机的 Ollama / LM Studio（免费）。在 AI 设置页可以点"获取模型列表"查看服务商当前可用的模型

## 快速开始

1. 运行安装包（或便携版）。程序没有代码签名，SmartScreen 弹出提示时点"更多信息 → 仍要运行"。
2. 引导：选择角色（消费者 / 商户 / 平台分析），然后连接 AI。如果本机装了 Ollama，会直接提示"使用本机模型（免费）"。
3. 选择平台：会按你所在的地区预选，并标出哪些平台需要登录。
4. 需要登录的平台，到「平台 · 账号与登录」点"登录"，在弹出的官网窗口里由你本人登录（扫码、短信、滑块都在窗口里完成）。软件确认账号页可以正常打开后才算登录成功，并在本机加密备份会话，之后长期复用。每个平台独立登录、互不影响。
5. 在「搜索」里用一句话描述需求，例如"Sony WH-1000XM5 300 欧以内 亚马逊德国 eBay 京东"；也可以直接粘贴一个商品链接，全网找同款比价。指定的平台里有没登录的，会先提示你登录。结果带店铺类型标签、风险评分和评论分析。
6. 也可以直接对 Copilot（Ctrl+J）说"用 app 搜索 …"，它会替你发起搜索，并在完成后点评结果。

建议首次使用时到「平台 · 平台目录」点"检测已启用平台"，再到「平台 · 账号与登录」点"登录体检"，确认各站点在你的网络和登录状态下都能正常识别。

从 v26.02 升级：直接安装覆盖即可，数据自动迁移；如果之前用的是 DeepSeek 的 `deepseek-chat`，启动后会自动换成 `deepseek-flash`。

## 校验文件

PowerShell：

```powershell
Get-FileHash .\Hawwwker-v26.03-windows-x64-setup.exe -Algorithm SHA256
```

把结果与 `SHA256SUMS.txt` 中对应的一行比对即可。

## 数据位置

- 安装版：`%APPDATA%\com.hawwwker.app\`（数据库 `hawwwker.db`、自动化浏览器的登录会话 `browser-profile\`、加密的会话备份（数据库内，Windows DPAPI 保护）、导出文件 `exports\`）；日志在 `%LOCALAPPDATA%\com.hawwwker.app\logs\`
- 便携版：exe 旁边的 `Hawwwker-data\`；程序本体解压缓存在 `%LOCALAPPDATA%\Hawwwker\portable\26.03\`

所有数据只保存在本机。除了在你的浏览器会话里打开购物网站、获取公开汇率之外，离开本机的只有发给你所配置的 AI 服务的需求文本和商品标题。

## 从源码构建

参见 `Hawwwker-v26.03-source.zip` 中的 `README.md`。发布文件由 `scripts/release.sh <版本>` 生成，命名规则固定为 `releases/v<版本>/Hawwwker-v<版本>-*`。
