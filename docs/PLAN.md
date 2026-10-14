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
| 设置窗口 | `settings_window.rs`，5 个面板（**v0.2 阶段 7.8 已纠正为 9 个**） | ✅ |
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

---

# v0.2 计划

v0.1.0 交付了完整的功能面（菜单 258 项 / 快捷键 72 条 / 动作 182 个），
但 UI 层是在未读 gpui-kit 两份规范的情况下写的。

v0.2 因此分成两条线：**先按规范纠正 UI，再补功能**。
纠正排在前面，因为功能会复用被纠正后的结构 —— 先加功能再改结构，等于写两遍。

## 阶段 7 — 按规范纠正 UI ✅

依据 [UI 审计报告](UI-AUDIT-v0.2.md)。

| 任务 | 内容 | 优先级 | 状态 |
| --- | --- | --- | --- |
| 7.1 `ElementId` 改用领域 ID | 标签关闭键 / 选择列表行 / 大纲项 | P0 | ✅ |
| 7.2 补滚动归属 | 侧栏与检查器 body，2 处 | P0 | ✅ |
| 7.3 补无障碍名 | 图标按钮加 `accessibility_label` + `tooltip` | P0 | ✅ |
| 7.4 选中语义改用语义组件 | 多选一控件改 `RadioGroup` / `TabBar::segmented()` | P1 | ✅ |
| 7.5 可点击 `div` 改语义组件 | 标签栏 / 侧栏 / 检查器 / Snippets 页签 | P1 | ✅ |
| 7.6 `px()` 字面量改 rem 档位 | 全部改 `rems()`；保留 3 类合法例外并注释 | P2 | ✅ |
| 7.7 大纲改用 `Tree` | 层级不再用缩进模拟 | P3 | ⏳ 推迟到阶段 8 |
| 7.8 设置窗口对齐上游 9 面板 | 见 §7.8 下方 | P0 | ✅ |

**验收标准**：
- 代码中 `px(` 只出现在三类合法例外处，且每处有注释说明理由；
- 所有 `ElementId` 都不含列表下标；
- 所有图标按钮都有无障碍名；
- 侧栏与检查器在内容溢出时可滚动；
- 多选一控件不再出现 `primary()`。

### 7.8 设置窗口重写（P0）

v0.1.0 的设置窗口是**照猜测写的**：上游有 9 个面板，SubText 只做了 5 个，
且把 Format 的换行符 / 编码 / 语法塞进了 General。详见审计报告 §9。

本轮：

1. `SettingsPane` 扩到 9 个变体，顺序、标签、图标逐一对齐
   （`SettingsPane::symbol()` 保留上游 SF Symbol 名，`icon_for()` 做映射）；
2. `Settings` 模型按上游 `@AppStorage` 键扩展为六个分组，
   字段名沿用上游键名，便于回溯；
3. 各面板按上游 `Grid(alignment: .leadingFirstTextBaseline)` 的
   「右对齐标签 + 左对齐控件」两列结构重写；
4. 无法兑现的开关禁用并说明原因，不假装生效；
5. 三个描述同一个缩进决定的字段收敛到 `Settings::set_indentation()` 一处写入。

**同轮发现的错误**：`gpui_kit::relative()` 是上一轮凭空写出的名字，并不存在；
正确的辅助函数是 `rems()`。11 处调用全部改正，详见审计报告 §11.1。

## 阶段 8 — 补齐高价值缺失功能 ⏳

从 [FEATURE-MATRIX](FEATURE-MATRIX.md) 的 39 项未实现中，按「用户可感知收益 ÷ 工作量」排序：

| 顺序 | 功能 | 理由 |
| --- | --- | --- |
| 8.0 | 大纲改 `Tree` | 阶段 7.7 顺延；`Tree` / `TreeState` 已提供 |
| 8.1 | 分屏编辑器 | 菜单里 3 项，`h_resizable`/`v_resizable` 已提供 |
| 8.2 | 文件夹查找 | 侧栏面板已在，缺查找逻辑 |
| 8.3 | 多重替换 | 需要一张可编辑的规则表 |
| 8.4 | 颜色代码编辑 | `ColorPicker` 组件已提供 |
| 8.5 | 按模式排序 | 纯逻辑，`subtext-core` 可测 |
| 8.6 | 选区扩展命令 | 纯逻辑（选包围符号、按行拆分） |
| 8.7 | 代码片段 | 需要编辑器与存储 |
| 8.8 | 快速操作 | 需要面板框架 |

**非目标（v0.2 仍不做）**：打印、AppleScript、书写方向 RTL、工具栏自定义。
理由：前三者需要平台能力或框架不支持，第四者收益低于成本。

## 阶段 9 — 验证与发布 v0.2.0 ⏳

| 任务 | 内容 | 状态 |
| --- | --- | --- |
| 9.1 | 全量测试 + clippy 零警告 | ✅（128 项 / 零警告） |
| 9.2 | 冒烟运行 + 打包 | ⏳ |
| 9.3 | 更新 CHANGELOG、README、FEATURE-MATRIX | ⏳ |
| 9.4 | 提交、打 `v0.2.0` 标签、重新打包 | ⏳ |

---

## 已知缺口（不阻塞，但需记账）

| 缺口 | 说明 |
| --- | --- |
| `cargo fmt` 未接入 | 全仓 21 个文件与 rustfmt 默认风格不一致（`actions!` 等宏调用的换行方式无法用 `rustfmt.toml` 对齐）。`menu.rs` / `keymap.rs` / `actions.rs` 是生成文件，要接入 fmt 得先让 `tools/generate_menu.py` 输出 rustfmt-clean 的代码。当前门禁是 **clippy 零警告 + 测试全绿**，不含 fmt。 |
| 大纲未用 `Tree` | 见阶段 8.0。 |
| Windows / Linux 未验证 | 只验证过 `aarch64-apple-darwin`。 |
| 冒烟测试的非 GUI 环境崩溃 | 10-14 23:34 有一份 `did_finish_launching` 的 SIGABRT 报告，发生在由 agent shell 以后台角色启动时。3 次复现未出现，未定位。 |

