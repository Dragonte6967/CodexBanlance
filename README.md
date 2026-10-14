# Codex Balance

Codex Balance 是一个 Windows 桌面小组件，用来查看当前 ChatGPT/Codex 账号的额度剩余情况。它使用 Tauri 2、Rust、TypeScript 和 Vite 编写，界面是浅色半透明无边框窗口，默认普通窗口层级，并通过系统托盘进行显示、隐藏、刷新、选择 CLI 和开机自启设置。

项目目标是交付 portable 单文件版本：发布目录只包含一个 `CodexBalance.exe`，不生成安装包，不附带 DLL、资源文件或配置文件。

## 功能

- 显示 Codex 额度剩余百分比、进度条和重置倒计时。
- 支持多个额度桶，下拉切换；默认优先显示 `codex` 桶。
- 启动后立即读取额度，之后每 60 秒刷新一次，并接收 app-server 更新通知。
- 手动刷新会自动合并请求，避免重复并发查询。
- 查询失败时保留上次数据并标记为过期。
- 未登录、缺少 CLI、超时或连接失败时显示中文提示。
- 无边框、无标题栏、无任务栏按钮，标题区域可拖动。
- 首次显示在主屏右上角，自动按屏幕分辨率缩放并记住位置。
- 点击桌面后会恢复到桌面窗口之上，但不会置顶或抢焦点；主动隐藏到托盘后不会自动弹回。
- 托盘菜单提供显示/隐藏、刷新、选择 Codex CLI、开机自启和退出。
- 退出时清理本组件启动的 `codex app-server` 子进程树。

## 运行要求

- Windows 10/11 x64。
- 已安装 Microsoft Edge WebView2 Runtime。
- 已安装 Codex CLI，并已通过 ChatGPT 账号登录。

小组件通过本机 `codex app-server --listen stdio://` 读取额度，复用 Codex CLI 的登录状态。它不会打包 WebView2、Codex CLI 或账号运行时，也不会自动安装缺失依赖。

如果尚未登录，请先在终端运行：

```powershell
codex login
```

CLI 查找顺序：

1. 托盘菜单中手动选择的原生 `codex.exe`。
2. `PATH` 中的 `codex.exe`。
3. `%USERPROFILE%\.bin\codex.exe`。

手动选择 CLI 时请选择真实的 `codex.exe`，不要选择 `.cmd` 包装脚本。

## 使用方式

直接运行 `CodexBalance.exe` 即可。默认不会开机自启；需要时可在托盘菜单中开启。开机自启登记的是当前 EXE 路径，如果移动了 EXE，请先关闭再重新开启自启。

窗口交互：

- 拖动标题区域移动窗口。
- 点击刷新按钮立即读取额度。
- 点击隐藏按钮或关闭窗口会隐藏到托盘。
- 托盘左键点击显示窗口。
- 托盘右键打开菜单。

窗口大小以 2560x1440 屏幕上的 500x200 实际像素为基准，其他屏幕按 `min(屏幕宽 / 2560, 屏幕高 / 1440)` 等比缩放。应用会尽量避免窗口因显示器断开、分辨率变化或 DPI 变化而留在不可见区域。

## 数据与隐私

应用设置和 WebView2 缓存位于：

```text
%LOCALAPPDATA%\local.codex.balance\
```

EXE 所在目录保持干净。普通运行不会写应用日志，不会把凭据传给前端，也不会把凭据写入日志或配置文件。Codex CLI 仍会使用它自己的用户数据目录。

额度计算规则：

- 剩余百分比为 `100 - usedPercent`。
- 结果限制在 `0..100`。
- 缺失数据会显示“暂无数据”。
- 重置倒计时使用服务端返回的 `resetsAt` 和本地时钟计算。
- 倒计时归零后等待服务端返回新数据，不会自行把额度改为 100%。

## 开发

需要 Node.js 22.12+、Rust MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK。

安装依赖：

```powershell
npm ci
```

开发运行：

```powershell
npm run tauri dev
```

测试：

```powershell
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

构建 portable 单 EXE：

```powershell
npm run portable
```

构建产物位于：

```text
release\CodexBalance.exe
```

`dist`、`src-tauri\target` 和 `release` 都是本机构建产物，不需要提交到仓库。

## Portable 边界

发布版满足以下目标：

- 前端资源和图标嵌入 EXE。
- Tauri bundle 关闭，不生成 MSI、NSIS 或其他安装包。
- MSVC CRT 和可静态链接的应用依赖使用静态链接。
- WebView2 Loader 使用静态链接。
- 发布目录只保留 `CodexBalance.exe`。

仍依赖系统自带或用户已安装的组件：

- Windows 系统 DLL。
- Windows 10/11 自带的 UCRT API DLL。
- Microsoft Edge WebView2 Runtime。
- 本机已安装并已登录的 Codex CLI。

可用 Visual Studio `dumpbin` 检查导入表：

```powershell
dumpbin /dependents release\CodexBalance.exe
```

检查目标是不要出现需要随应用分发的 `VCRUNTIME*.dll`、`MSVCP*.dll` 或 `WebView2Loader.dll`。

## 诊断

开发验收时可以运行：

```powershell
release\CodexBalance.exe --smoke-test
```

该模式会显示真实窗口并读取真实额度，约 25 秒后把不含凭据的窗口状态和额度快照写入用户数据目录下的 `smoke-report.json`，然后退出。不要在已有实例运行时使用此模式，因为单实例逻辑会优先唤起旧实例。

常见问题：

- 提示找不到 CLI：确认 `codex.exe` 在 `PATH` 中，或从托盘菜单手动选择。
- 提示未登录：运行 `codex login` 后重新刷新。
- 窗口没有出现：检查托盘图标，或退出旧实例后重新打开。
- 移动 EXE 后自启失效：关闭托盘中的自启选项，再重新开启。

## 项目结构

```text
src/                 前端 TypeScript 与样式
src-tauri/src/       Rust 后端、窗口、托盘、Codex app-server 客户端
src-tauri/icons/     内嵌图标
tests/               前端模型测试
scripts/             portable 打包、图标生成和诊断辅助脚本
```

核心接口依据：[Codex App Server](https://learn.chatgpt.com/docs/app-server)。
