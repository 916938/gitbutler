# 变更日志：WebView2 资源占用修复

**版本**: `0.22.0+deepseek`  
**提交**: `6f756f2422` (amend 自 `b958762f3c`)  
**日期**: 2026-08-07  
**分支**: `master`  
**构建类型**: Release (optimized)

---

## 问题描述

在 Windows 平台上启动 GitButler 时，WebView2 创建失败，报错：

```
failed to create webview: WebView2 error: WindowsError(Error {
    code: HRESULT(0x800700AA),
    message: "The requested resource is in use."
})
```

`0x800700AA` (ERROR_BUSY) 表示 WebView2 默认用户数据目录
(`%LOCALAPPDATA%\com.gitbutler.app.dev\EBWebView`) 被其他使用 WebView2 的
应用程序（如 Edge 浏览器）锁定，导致 GitButler 无法创建窗口。

## 根因分析

Tauri 默认使用 `%LOCALAPPDATA%\com.gitbutler.app.dev` 作为 WebView2 数据目录
（见 `tauri-2.11.4/src/manager/webview.rs:534-545`）。所有使用 WebView2 的
应用共享相同的运行时资源，当多个应用同时使用同一数据目录时，会产生资源竞争。

## 修复方案

在 `WebviewWindowBuilder` 中通过 `data_directory()` 方法显式指定一个独立的
`webview2-data` 子目录，将 GitButler 的 WebView2 实例与其他应用隔离。

### 修改文件

#### 1. `crates/gitbutler-tauri/src/window.rs` (核心修复)

**修改函数**: `create()` (非 macOS 分支)

**改动内容**:

- 将 `WebviewWindowBuilder::new(...).build()` 拆分为 builder 构建和 build 两步
- 在 Windows 平台 (`#[cfg(target_os = "windows")]`) 通过 `data_directory()`
  设置独立的数据目录
- 数据目录路径: `%LOCALAPPDATA%\com.gitbutler.app.dev\webview2-data`
- 添加 `use tauri::Manager` 导入以使用 `handle.path().app_local_data_dir()` API

**影响范围**: 仅 Windows 平台 (`#[cfg(target_os = "windows")]`)，不影响 macOS 和 Linux

#### 2. `apps/desktop/src/lib/ai/openAIClient.ts` (类型修复)

**改动内容**:

- 将 `DeepSeekModelName` 添加到 `OpenAIClient` 构造函数 `modelName` 参数的类型中
- 原类型: `OpenAIModelName | OpenRouterModelName`
- 新类型: `OpenAIModelName | OpenRouterModelName | DeepSeekModelName`
- 同步更新了私有字段 `self.modelName` 的类型声明

**原因**: 上游合并后 `DeepSeekModelName` 枚举值不是 `${string}/${string}` 格式，
无法隐式匹配 `OpenRouterModelName` 类型，导致 TypeScript 编译错误

#### 3. `package.json` + `pnpm-lock.yaml` (开发依赖)

**改动内容**:

- 添加 `@vitest/coverage-v8@3.2.6` 到 workspace root devDependencies
- 用于生成单元测试覆盖率报告

#### 4. `apps/desktop/src/lib/ai/openAIClient.test.ts` (新增单元测试)

**改动内容**:

- 新增 8 个测试用例，覆盖 DeepSeek 类型兼容性修复
- 测试分组: DeepSeek 构造 (3 个)、OpenAI 构造 (2 个)、OpenRouter 构造 (1 个)、evaluate 方法 (2 个)
- Mock OpenAI SDK，验证 baseURL 传递和 model 名称传递

#### 5. `packages/ui/src/lib/utils/testIds.test.ts` (新增单元测试)

**改动内容**:

- 新增 4 个测试用例，验证上游合并后 TestId 枚举完整性
- 测试 `BranchHeaderContextMenu_CreatePR` 和 `BranchHeaderContextMenu_Land` 存在且值正确
- 验证所有枚举值唯一且非空

#### 6. `@gitbutler/ui` 包重建 (类型同步)

**改动内容**:

- 上游合并后 `packages/ui/dist/` 中的类型定义过时，缺少 `BranchHeaderContextMenu_CreatePR`、`BranchHeaderContextMenu_Land` 和 `SelectItem.hoverIcon` 属性
- 通过 `pnpm --filter @gitbutler/ui run package` 重建 dist 目录解决
- 无源码改动，仅重新生成编译产物

## 单元测试覆盖率

| 文件              | Statements     | Branches  | Functions  | Lines  |
| ----------------- | -------------- | --------- | ---------- | ------ |
| `openAIClient.ts` | 88.57% (31/35) | 60% (3/5) | 100% (3/3) | 88.57% |
| `testIds.ts`      | 100%           | 100%      | 100%       | 100%   |

> `openAIClient.ts` 未覆盖的 4 条语句是 `evaluate` 方法中 `for await` 异步迭代循环体 (mock 返回空数组不产生 chunk)。

## 验证结果

### Debug 构建验证

| 验证项               | 修复前                                 | 修复后                |
| -------------------- | -------------------------------------- | --------------------- |
| WebView2 创建        | 失败 (0x800700AA)                      | 成功                  |
| 进程内存             | 43 MB (仅 Rust 后端)                   | 68 MB (含 WebView2)   |
| 子进程数             | 0 (无 WebView2)                        | 2 (WebView2 渲染进程) |
| 环境变量             | 需手动设置 `WEBVIEW2_USER_DATA_FOLDER` | 无需任何配置          |
| `cargo check`        | -                                      | 通过 (0 错误)         |
| `cargo build`        | -                                      | 通过 (5 分钟)         |
| `svelte-check`       | -                                      | 通过 (0 错误, 0 警告) |
| `pnpm build:desktop` | -                                      | 通过                  |

### Release 构建验证

| 验证项                                   | 结果                                          |
| ---------------------------------------- | --------------------------------------------- |
| `cargo build --release`                  | 通过 (7m16s)                                  |
| exe 大小                                 | 71.2 MB (debug: 92.8 MB, 减少 23%)            |
| WebView2 数据目录                        | 已创建 (`webview2-data/`, 54.2 MB, 392 文件)  |
| WebView2 进程数                          | 6 个 (msedgewebview2), 总内存 ~267 MB         |
| ERROR_BUSY (0x800700AA)                  | 未出现                                        |
| 应用窗口                                 | 正常创建, 响应正常                            |
| `webview2-data` 字符串验证               | exe 中包含 (修复已编译)                       |
| `but.exe` release 构建                   | 通过 (5m53s), `but --version` / `--help` 正常 |
| `gitbutler-git-askpass.exe` release 构建 | 通过                                          |

## 副作用评估

| 方面               | 影响                                                                              |
| ------------------ | --------------------------------------------------------------------------------- |
| 其他 WebView2 应用 | 无影响 (仅修改 GitButler 自身)                                                    |
| macOS / Linux      | 无影响 (`#[cfg(target_os = "windows")]` 条件编译)                                 |
| 用户配置           | 无需任何手动配置                                                                  |
| 磁盘空间           | 新增 `webview2-data` 目录 (~80MB, WebView2 缓存)                                  |
| 已有用户数据       | 旧 `EBWebView` 目录中的数据不会被迁移,但应用数据存储在 Rust 端 SQLite 中,不受影响 |

## 相关提交

| 提交         | 描述                                                                                       |
| ------------ | ------------------------------------------------------------------------------------------ |
| `6f756f2422` | 本次修复 (amend): WebView2 数据目录 + DeepSeek 类型 + coverage 依赖 + 单元测试 + CHANGELOG |
| `b958762f3c` | 原始提交 (已被 amend 替换)                                                                 |
| `f60862b99c` | PR #1: Windows PowerShell LOCALAPPDATA 运行时解析                                          |
| `42ad444a97` | PR #2: DeepSeek AI provider 支持                                                           |

---

## 发布包信息

**文件**: `dist/gitbutler-0.22.0-deepseek-windows-x64.zip`  
**版本**: `0.22.0+deepseek`  
**大小**: 49.0 MB (压缩后)  
**平台**: Windows x86_64  
**构建时间**: 2026-08-07 06:26

### 包含文件

| 文件                        | 大小    | 构建类型 | 说明                      |
| --------------------------- | ------- | -------- | ------------------------- |
| `gitbutler-tauri.exe`       | 67.9 MB | Release  | 主程序 (含 WebView2 修复) |
| `but.exe`                   | 53.5 MB | Release  | CLI 工具                  |
| `gitbutler-git-askpass.exe` | 147 KB  | Release  | Git askpass 工具          |
| `CHANGELOG-webview2-fix.md` | 7 KB    | -        | 变更日志                  |

> 全部三个可执行文件均为 Release (optimized) 构建，由 `build-release.ps1` 生成。
> 版本号来源为 `crates/gitbutler-tauri/Cargo.toml`，需升版本时请手动编辑该文件。

### 安装方式

1. 解压 zip 到任意目录
2. 将解压目录添加到系统 PATH（可选，用于 `but` CLI 命令）
3. 直接运行 `gitbutler-tauri.exe` 启动应用

### 系统要求

- Windows 10/11 x64
- WebView2 Runtime (Windows 11 已内置, Windows 10 可能需安装)
