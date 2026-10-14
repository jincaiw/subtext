# SubText 实施计划

> 配套文档：[评估报告](EVALUATION.md) · [架构说明](ARCHITECTURE.md) · [功能对照表](FEATURE-MATRIX.md)

---

## 1. 总策略

把 CotEditor 当作**规格说明**，而不是当作代码来翻译。

因此顺序是：**先固化数据 → 再搭骨架 → 最后填行为**。

理由：菜单、快捷键、语法、主题这四样是「数据」，可以机器提取、逐字节比对，
是保真度的地基；如果先写行为，后面每加一个菜单项都要回头改结构。

---

## 2. 阶段划分

### 阶段 0 — 环境与参考基线 ✅

| 任务 | 产出 | 状态 |
| --- | --- | --- |
| 验证工具链 | Rust 1.96.1 / arm64 | ✅ |
| 克隆参考仓库 | `.refs/gpui-kit`、`.refs/CotEditor` | ✅ |
| 建立工作区 | 4 个 crate 的 Cargo workspace | ✅ |

### 阶段 1 — 数据提取与固化 ✅

| 任务 | 产出 | 状态 |
| --- | --- | --- |
| 提取菜单表 | `resources/menu.json`（258 项 / 28 路径 / 168 命令 / 52 分隔符） | ✅ |
| 提取快捷键表 | `resources/menu.json` 的 `mods` + `key` 列 | ✅ |
| 复刻 `.cotsyntax` 解析 | `subtext-syntax`，70 个 bundle 全部可读 | ✅ |
| 复刻 `.cottheme` 解析 | `subtext-theme`，14 个主题全部可读 | ✅ |
| 复刻高亮规则优先级 | `subtext-syntax::highlight`，10 种 token | ✅ |

**验收标准**：`cargo test -p subtext-syntax -p subtext-theme` 全绿，且能读遍
`resources/` 下所有文件。

### 阶段 2 — 核心逻辑 ✅

| 任务 | 产出 | 状态 |
| --- | --- | --- |
| 编码与 BOM | `subtext-core::encoding`（6 种 BOM、编码注册表、探测回退链） | ✅ |
| 换行符 | `subtext-core::line_ending`（LF/CRLF/CR/NEL/LS/PS） | ✅ |
| 文档模型 | `subtext-core::document` | ✅ |
| 文本操作 | `subtext-core::textops`（30+ 个操作） | ✅ |
| 查找引擎 | `subtext-core::finder`（正则、全字、循环、反向） | ✅ |
| 字符统计 | `subtext-core::charinfo` | ✅ |
| 大纲提取 | `subtext-app::outline`（复刻 4 条隐式语义） | ✅ |

**验收标准**：`subtext-core` 56 项 + `subtext-syntax` 29 项 + `subtext-theme` 7 项测试全绿。

### 阶段 3 — UI 骨架 ✅

| 任务 | 产出 | 状态 |
| --- | --- | --- |
| 动作词表 | `actions.rs`，182 个类型化动作（10 个命名空间） | ✅ |
| 菜单生成 | `tools/generate_menu.py` → `menu.rs` | ✅ |
| 快捷键生成 | 同生成器 → `keymap.rs`（72 条 + 72 行显示表） | ✅ |
| 运行时接线 | `bridge.rs`，动作 → 命令名的单点映射 | ✅ |
| 主窗口 | `workspace.rs`（标签栏、编辑器、侧栏、检查器、状态栏） | ✅ |
| 高亮接入 | `highlighter.rs`，`.cotsyntax` 规则接到 GPUI 的 `InputHighlighter` | ✅ |
| 设置窗口 | `settings_window.rs`，5 个面板 | ✅ |
| 关于窗口 | `dialogs.rs` | ✅ |

**验收标准**：应用能启动、能开文件、能高亮、菜单可点。

### 阶段 4 — 编译、测试与运行验证 ✅

| 任务 | 产出 | 状态 |
| --- | --- | --- |
| 首次编译 | 从 54 个错误降至 0 | ✅ |
| 全量测试 | 124 项（32 + 56 + 29 + 7）全绿 | ✅ |
| clippy | `--workspace --all-targets` 零警告 | ✅ |
| 启动冒烟 | `./target/debug/subtext` 稳定存活 | ✅ |
| 命令行开文件 | `./target/debug/subtext /tmp/subtext-demo.rs` | ✅ |

### 阶段 5 — 文档 ✅

| 任务 | 产出 | 状态 |
| --- | --- | --- |
| 评估报告 | `docs/EVALUATION.md` | ✅ |
| 实施计划 | `docs/PLAN.md`（本文） | ✅ |
| 架构说明 | `docs/ARCHITECTURE.md` | ✅ |
| 功能对照表 | `docs/FEATURE-MATRIX.md` | ✅ |

### 阶段 6 — 提交与发布 ✅

| 任务 | 产出 | 状态 |
| --- | --- | --- |
| 仓库初始化 | `git init -b main` + `.gitignore`（排除 `.refs/`、`target/`、`dist/`） | ✅ |
| 首次提交 | 345 个文件，42 个非资源文件 | ✅ |
| 打标签 | 注解标签 `v0.1.0` | ✅ |
| 图标 | `tools/make_icon.py`，纯标准库绘制 7 种尺寸 → `.icns` | ✅ |
| Release 构建 | `cargo build --release`，1 分 51 秒，产物 21 MB | ✅ |
| macOS 打包 | `tools/package-macos.sh` → `dist/SubText.app` + zip（8.5 MB） | ✅ |
| 自包含验证 | 隐藏源码树 `resources/` 后 bundle 仍能启动 | ✅ |
| 发布说明 | `CHANGELOG.md` | ✅ |

---

## 3. 跨平台验证计划

| 目标 | 状态 | 说明 |
| --- | --- | --- |
| `aarch64-apple-darwin` | ✅ 已验证 | 开发机，编译 + 测试 + 运行 |
| `x86_64-apple-darwin` | ⏳ 待验证 | 需要 `rustup target add` |
| `x86_64-pc-windows-msvc` | ⏳ 待验证 | 需要 Windows 主机或交叉工具链 |
| `x86_64-unknown-linux-gnu` | ⏳ 待验证 | 需要 Linux 主机或交叉工具链 |

**已知平台差异点**（代码中已用 `#[cfg]` 处理）：

- 等宽字体：macOS `Menlo` / Windows `Consolas` / Linux `DejaVu Sans Mono`
  （`dispatch.rs::mono_font()`）。
- `secondary` 修饰键：macOS = Command，Windows/Linux = Control（由 GPUI 处理）。
- 拼写 / 语音 / AppleScript：仅 macOS，其余平台走「不可用」提示。

---

## 4. 明确的非目标（v0.1.0）

这些**不阻塞** 0.1.0 发布，菜单项保留但触发时提示未实现：

- 打印与页面设置
- AppleScript 字典
- 拼写检查 / 语法检查 / 语音朗读
- 文件夹查找（Folder Find）
- 多重替换（Multiple Replace）
- 分屏编辑器
- 代码片段（Snippets）
- 快速操作（Quick Actions）
- 颜色代码编辑
- 书写方向（RTL）
- 选区扩展命令（Select Enclosing Symbols 等）

详见 [FEATURE-MATRIX.md](FEATURE-MATRIX.md)。
