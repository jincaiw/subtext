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

| 顺序 | 功能 | 理由 | 状态 |
| --- | --- | --- | --- |
| 8.0 | 大纲改 `Tree` | 阶段 7.7 顺延；`Tree` / `TreeState` 已提供 | ⏳ |
| 8.1 | 分屏编辑器 | 菜单里 3 项，`h_resizable`/`v_resizable` 已提供 | ✅ |
| 8.2 | 文件夹查找 | 侧栏面板已在，缺查找逻辑 | ✅ |
| 8.3 | 多重替换 | 需要一张可编辑的规则表 | ✅ |
| 8.4 | 颜色代码编辑 | `ColorPicker` 组件已提供 | ✅ |
| 8.5 | 按模式排序 | 纯逻辑，`subtext-core` 可测 | ✅ |
| 8.6 | 选区扩展命令 | 纯逻辑（选包围符号、按行拆分） | ✅ |
| 8.7 | 代码片段 | 需要编辑器与存储 | ⏳ |
| 8.8 | 快速操作 | 需要面板框架 | ⏳ |

**非目标（v0.2 仍不做）**：打印、AppleScript、书写方向 RTL、工具栏自定义。
理由：前三者需要平台能力或框架不支持，第四者收益低于成本。

### 8.1 分屏编辑器 ✅

上游 *View* 菜单里三条命令此前都只回一句「未实现」：*Split Editor*、*Close Split Editor*、
*Stack Editors Vertically*（三者都无快捷键），外加 *Move Focus to Next / Previous Editor*
（⌃\` / ⇧⌃\`）。

CotEditor 的实现是 `DocumentViewController`（一个 `NSSplitViewController`）：每个窗口最多
**4** 个编辑器，全部共享同一个 `NSTextStorage`，因此一处输入立刻出现在所有分屏里；
每个 `EditorTextView` 各有自己的选区、滚动位置、字体、抗锯齿、连字、主题、tab 宽度与
自动展开开关，新建分屏时从基准视图**复制**一份，并把它已经算好的语法高亮结果也拷过去，
省掉一次重新解析。`openSplitTextView` 把新分屏插在当前分屏**之后**、继承选区、把两边都
滚到该选区、把焦点交给新分屏；`closeSplitTextView` 把焦点交给**前一个**分屏，关掉第一个
时交给**最后一个**（上游用 `textViews[safe: deleteIndex - 1] ?? textViews.last!`，
在删除前的数组上取，所以关掉第 0 个时拿到的是原数组的末位）。`toggleSplitOrientation`
翻转 `splitView.isVertical` 并把结果写进 `UserDefaults.splitViewVertical`（默认 `false`，
即上下堆叠）。

SubText 的模型不同：一个窗口装多个标签页，每个 `OpenDocument` 原本只有**一个**
`Entity<EditorState>`。所以这轮先把「一个文档一个编辑器」改成「一个文档一组编辑器」：

1. **`OpenDocument` 从 `editor` 变成 `panes: Vec<Pane>` + `active_pane: usize`**，
   并给出 `editor()`（当前焦点分屏）、`editors()`（全部，按视觉顺序）、`pane_count()`、
   `active_pane()`、`is_split()`。`Pane` 把编辑器与**它自己的**订阅绑在一起，
   于是关掉一个分屏会精确地释放它那一条订阅，其余不受影响。
   原先的 `_subscriptions: Vec<Subscription>` 随之取消。
2. **文本同步**：GPUI 的每个 `EditorState` 各持一份 `Rope`，没有共享文本存储，
   所以由工作区自己把文本铺开 —— `sync_panes()`。一个分屏的 `Change` 到达订阅后，
   `on_pane_changed()` 先判断这条 `Change` 是不是「同步的回声」：如果该分屏的文本
   已经等于文档模型的文本，就直接返回。这一条判断是**唯一**的终止条件，也是整个
   同步不会来回震荡的原因。真正的改动才写回文档模型、重探行尾、清掉查找结果，
   然后把新文本**延迟**（`window.defer`）写进其余分屏 —— 与其它所有程序化写入同样的
   理由（`replace_all` 会发 `Change`，而这个函数正跑在由源分屏的 `Change` 起头的
   工作区更新里）。
3. **选区与滚动位置**：`replace_all` 会把选区重置到 `0..0` 并把滚动条拉回顶部，
   而 CotEditor 的分屏各有各的选区，不该因为别人打字就跳走。所以写之前记下
   `selected_range()` 与 `scroll_offset()`，写完立刻放回去（`set_selected_range`
   自己会做边界裁剪，文本变短也不会越界）。
4. **渲染**：`render_editor_area()`。单分屏时就是一个编辑器；多分屏时套进
   `v_resizable` / `h_resizable`（按 `split_view_vertical` 选轴），每个分屏是一个
   `resizable_panel()`，分隔条可拖动。分组用 `ElementId::NamedInteger("editor-split", 文档 id)`
   作键，于是每个文档各记各的分隔位置 —— 对应上游每个窗口各自 autosave 的 `NSSplitView`。
   每个分屏外面包一层带 id 的 `div`，挂一个 `on_mouse_down` 把它标成当前分屏并聚焦，
   这就是「点击决定命令作用于哪一半」。
5. **编辑器级设置广播**：`apply_settings()` 与 `set_theme()` 从「每个文档的编辑器」
   改成「每个文档的每个分屏」；`set_line_ending` / `change_encoding` /
   `reinterpret_encoding` / `revert` / `set_syntax` / `rehighlight` 改成遍历**当前文档
   的全部**分屏 —— 与上游那几个 `for textView in self.textViews` 循环一一对应。
   这些走 `set_value`，它不发 `Change`，所以同步通道不会被误触发。
6. **落库**：`Settings::split_view_vertical`（默认 `false`）。上游放在 `UserDefaults`
   且**没有设置界面**，SubText 同样不给开关，只由菜单项与导航栏按钮改。

**接线**：`FocusNextEditor` / `FocusPreviousEditor` / `OpenSplitTextView` /
`CloseSplitTextView` / `ToggleSplitOrientation` 从「提示未实现」改成调用真方法。

**已知偏差（记账）**：

1. **撤销栈按分屏而非按文档**。上游共享一个 `NSTextStorage`，撤销天然是文档级的；
   SubText 每个 `EditorState` 有自己的撤销栈，同步写入经 `replace_all` 记录进目标分屏的
   栈里，所以在任一分屏按 ⌘Z 都会产生一次改动、再同步回其余分屏。行为自洽，但
   「撤销是文档的」这一点不再成立。改用 `set_value` 可以避免栈里多出条目，代价是目标
   分屏的撤销历史被清空（`set_value` 会 `undo_manager.clear()`），那更糟。
2. **光标按绝对偏移保留，而不是按编辑增量平移**。共享文本存储会自动把其它视图的
   插入点往后推；SubText 是整篇替换后恢复原偏移，所以在一处插入字符时，另一分屏里
   位于其后的光标不会跟着 +1。视觉上不可见（另一分屏的光标通常不在视野里），
   但严格说与上游不同。
3. **⌃\` / ⇧⌃\` 在未分屏时退化为切换标签页**。上游此时是空操作（重新聚焦同一个编辑器）。
   SubText 的标签页就是它的窗口，而那两条键原本就是切标签页用的，改成空操作等于
   删掉一个已有功能，所以保留：分屏了就轮换分屏，没分屏就切标签页。
4. **没有 ⌃⇥ 之类的分屏快捷入口**。上游分屏只能从菜单或导航栏按钮发起，SubText 相同；
   导航栏上的 *Split Editor* / *Close Split Editor* / *Stack Editors* 三个按钮本轮未加
   （SubText 的导航栏目前只有面包屑与大纲菜单），菜单项已可用。
5. **分隔条位置不落盘**。上游 `NSSplitView` 用 autosave 记住每个窗口的分隔比例；
   SubText 的 `ResizableState` 活在窗口的 keyed state 里，同一次运行内切换标签页会保留，
   重启后回到等分。
6. **`test-support` 未启用**，分屏的 UI 无法无头驱动。可测的两条规则被抽成纯函数
   （`successor_pane()`、`cycled_pane()`）并各有测试；同步通道用一次临时冒烟验证覆盖：
   启动时强制分屏，向第 0 个分屏写入 `"hello world"`，两秒后读出两个分屏与文档模型 ——
   三者一致且文档被标脏，随后已还原该补丁。

### 8.6 选区扩展命令 ✅

上游 Select 菜单里三条命令，此前全是占位提示。本轮补齐：

1. `subtext-core::textops` 新增 `SymbolPair` 与 `range_of_enclosing_symbol_pair()`，
   用 `PairScanner` 两趟扫描复刻上游 `SymbolPairScanner`：前向找未配对的闭合符，
   后向只在前向选定的那一对上找配对的开符。候选集为 `SymbolPair::ENCLOSING`
   （`()` `{}` `[]` `<>`），与上游 `SymbolPair.braces + [.ltgt]` 一致。
   索引按 **字符** 而非字节计算（上游 `Character` 语义）。
2. 新增 `line_content_ranges()`：按上游 `String.lineContentsRanges(for:)` 的
   「逻辑行内容范围、不含行尾」语义拆分选区，行尾识别覆盖 `\n` / `\r\n` / `\r`。
3. `Workspace` 新增 `select_enclosing_symbols()` / `split_selection_by_lines()` /
   `insert_text()`；后两者用 `set_selected_range` + `add_selection` 落多光标，
   插入走 `window.defer` 以避开 Change 事件重入。
4. 接线 4 个动作：`SelectEnclosingSymbols`、`SplitSelectionByLines`、
   `InputBackSlash`（⌘¥）、`InputYenMark`（⌘Y）。

**明确不做的**：`SelectColumnUp` / `SelectColumnDown`（⇧⌃↑ / ⇧⌃↓）。上游靠 TextKit
的 layout manager 拿**视觉行**（`numberOfWrappedRows` / `lineFragmentRect`），
gpui-kit 编辑器只暴露逻辑行，无法复刻；改为提示而非假装生效。

### 8.4 颜色代码编辑 ✅

上游 `Edit Color Code`（菜单 *Format → Edit Color Code*，无快捷键）打开系统调色板
并挂一个 accessory view：一个等宽文本框放颜色代码、一个十一种记法的下拉菜单、
一个 *Insert* 按钮。选色改写文本框，在文本框里敲代码回写调色板，*Insert* 把代码
插到插入点。

SubText 没有系统调色板可挂，于是面板与 accessory 合成一个对话框：
`crates/subtext-app/src/color_sheet.rs`，用 gpui-kit 的 `ColorPicker` 顶替色轮。

1. **转换逻辑整体移植**：`subtext-core/src/colorcode.rs` 是上游 `WFColorCode`
   （MIT，作者同为 1024jp）的移植，11 种记法的**解析与格式化字符串逐字符对齐**。
   - 分量一律 `0.0..=1.0`，色相是**圈数的分数**而非角度——与 GPUI 的 `Hsla`
     同域，所以送进组件时不发生换算。
   - 识别顺序即 `ColorCodeType::ALL` 的顺序，先匹配先赢：`#ffffff` 是 `Hex`
     而不是 `HexWithAlpha`，只是因为 `Hex` 先试。
   - 148 个 CSS 关键字按上游顺序排列——顺序有语义，`#00ffff` 报 `aqua` 而不是
     靠后的 `cyan`，`#ff00ff` 报 `fuchsia` 而不是 `magenta`。
   - alpha 的写法复刻 `fractionLength(0...2)`：最多两位小数、去掉末尾零、
     **逢半进位**（Rust 的 `{:.2}` 是逢半取偶，所以先自己舍入再格式化）。
2. **面板行为对齐**：文本框用等宽字体；记法下拉分 *Hexadecimal* / *CSS* 两段
   （`SearchableVec<SelectGroup<&'static str>>`）；短十六进制或关键字无法表示该颜色时
   退回 `Hex` 并**同步移动菜单**，不让面板显示一个它没在用的记法。
3. **字母大小写保留**：上游保留打开面板时代码的大小写（`#FF6600` 写回 `#FF6600`）。
   标志位在每次编辑时从文本框内容重算。
4. **落库**：`Settings::color_code_type` 存 CotEditor 的 `colorCodeType` 原值
   （默认 `1` = `Hex`）；上一次的颜色放在 `Workspace::last_color_code_color`，
   对应上游那个会话级的共享调色板状态，不落盘。

**已知偏差（记账）**：上游的调色板是非模态浮动面板，插入时插入点可以移动；
SubText 用对话框，因此面板打开期间无法改动插入点。面板内其余行为一致。

### 8.5 按模式排序 ✅
上游 `Pattern Sort`（⇧⌘S）此前只有「升序排序」一条，且排序引擎是自造的简化版。
本轮按上游 `SortPattern` 模型整体重写：

1. `SortKey` 三变体对齐上游三个 key：
   - `EntireLine` — 整行；
   - `Column { delimiter, column }` — 分隔符 + 1-based 列号，字段做
     `trimmingCharacters(in: .whitespaces)`，字段不存在返回 `None`（排到末尾），
     分隔符经 `finder::unescape()` 展开、空串回退为 `,`；
   - `RegularExpression { pattern, ignore_case, group }` — 可选捕获组，
     `ignore_case` 用 `(?i)` 前缀实现，`validate()` 用 `captures_len() - 1` 校验组号上界。
2. `SortOptions` 四开关对齐上游：`ignores_case` / `numeric` / `keeps_first_line` /
   `descending`（默认值与上游一致：前三者为 `true/true/false`）。
   `numeric` 的 `leading_number()` 复刻 ICU 分组规则（分隔符后须恰好 3 位数字）。
3. `LogicalLine { contents, line_ending }` 保留**每行自己的行尾**，
   `join_logical_lines()` 按上游 `joined(baseLineEnding:includingTrailingLineEnding:)`
   拼回；`sort_lines()` 要求 `lines.count > 1` 且有 base line ending，否则返回空 `Edit`。
4. `span_for()` 为 sheet 的样例预览提供高亮范围。
5. 新建 `crates/subtext-app/src/sort_sheet.rs`（sheet UI）：三 key 单选 +
   各 key 的参数区 + 四个开关 + 一个样例预览（按 `span_for` 高亮三行）+ 底部按钮；
   默认停在 **Column**（与上游一致）。
6. `pattern_sort_dialog()` 用 `window.open_dialog` 挂 `SortSheet`，
   Enter 与底部 Sort 按钮都走 `submit()`。

**已知偏差（记账）**：上游 *Respect language rules* 开关用
`KeyPathComparator(\.contents, comparator: .localized)`，依赖 ICU 的 locale 感知排序；
Rust 标准库无 ICU，该开关在 sheet 里渲染为 **disabled** 并注明原因，不假装生效。

### 8.2 文件夹查找 ✅

上游 *Find ▸ Find in Folder…*（⇧⌘F）把「查找栏」换成「文件夹查找」侧栏：一个搜索框、
一个 *Search method* 下拉（Text / Regular Expression）、一个 *Case Sensitive* 开关、
一个文件范围菜单（隐藏文件、其他文件类型、编辑范围、清除范围、已存范围），
下面是进度行与按文件分组的结果树。点结果在新窗口打开命中位置。

本轮分两层落地。

1. **模型**（`subtext-core/src/folderfind.rs`，约 1830 行含测试，移植自
   `FolderFind.swift` / `Search.swift` / `FileScope.swift` / `FolderFindProgress.swift`
   / `URL+Binary.swift`）：
   - `Query { find_string, mode }`，`Mode { uses_regular_expression, ignores_case }`。
     **默认是大小写敏感**（两个标志都为 `false`），与上游 `FolderFindMode` 一致；
     `Query::searcher()` 是唯一的编译入口，空串报 `EmptyFindString`、
     正则不编译报 `InvalidRegularExpression`。
   - `Options` 七个字段对齐上游 `FolderFindOptions`：默认 `includes_other_file_types
     = false`、`includes_hidden_files = false`、`excluded_names = [".DS_Store", ".git"]`、
     `maximum_file_size = 1_000_000_000`、`maximum_match_count = 500_000`、
     `maximum_match_count_per_file = 5_000`、`file_scope = None`。
   - `Search::run(progress, cancel)` 遍历目录：读目录 → 剔除 `excluded_names`、
     隐藏项与符号链接 → 排序（**目录优先**，同类内用 `natural_compare`，
     所以 `file2` 排在 `file10` 前面）→ 递归。命中数到上限即停并置
     `has_unsearched_files`；`Cancellation` 是 `Arc<AtomicBool>`，在目录、文件、
     每行三个层级各查一次，被取消返回 `Err(Cancelled)`。
   - `Match { range, line, range_in_line }` 的 `range` 是**文档内字节偏移**，
     `range_in_line` 是**行内字节偏移**。行长超过 `MAXIMUM_LINE_LENGTH`（512）时
     以命中为中心裁窗口，命中前保留 `HEAD_PADDING`（64）字节；
     `grapheme_range` 用 `unicode-segmentation` 的 `grapheme_indices(true)` 复刻
     Foundation 的 `rangeOfComposedCharacterSequences`，保证裁剪点落在字素簇边界，
     不会把 emoji 或组合字符劈开。
   - `FileScope { conjunction, rules }` 是本轮唯一新加 `Serialize`/`Deserialize`
     的模型类型（为了落盘）：`Conjunction`（Any / All）、`Rule { target, comparison,
     value }`、`Target`（Filename / FilePath / FileExtension）、
     `Comparison`（七种，含 `MatchesRegularExpression`）。
     `validate()` 拒绝空值与不编译的正则；正则规则编译成
     `\A(?:{值}(?x)\n)\z` —— 包 `(?x)` 让模式里的空白被忽略、再用
     `\A…\z` 锚定整串，从而等价于上游 `NSRegularExpression` 的
     「整串匹配 + `allowCommentsAndWhitespace`」。
   - 二进制识别 `is_binary()` 复刻 `URL+Binary.swift`：读前 8 KiB 找 NUL，
     找不到 NUL 就按 UTF-8 解码率判定；`is_searchable_text()` 是**扩展名表**
     判定（见下面的偏差 3）。
   - 39 项测试逐条对齐上游测试名（`naturalCompare`、行裁剪、范围裁剪、
     字素边界、作用域七种比较、取消、截断、排除项、隐藏项、目录优先排序…）。

2. **面板**（`crates/subtext-app/src/folder_find.rs`，约 2000 行含测试）：
   - `FolderFindPane` 持有 `InputState`、三个开关、`FileScope` 与
     `BTreeMap<String, FileScope>`（已存范围）、`root: Option<PathBuf>`、
     `state: SearchState`、`submitted: String` 与 `generation: u64`。
     `SearchState` 四态 `Idle / Searching { progress, cancel } / Finished(Summary)
     / Failed(String)`。
   - **扫描前把能报的错都报掉**，顺序与上游一致：空串 → 回 `Idle`；
     根目录不存在或不是目录 → `Failed("The folder cannot be found.")`；
     `Query::searcher()` 失败 → 该错误的消息；`scope.validate()` 失败 → 该错误的消息。
     只有全过才起搜索，避免一个注定失败的任务先闪一下 spinner。
   - 搜索跑在 `cx.background_executor()` 上，`generation` 计数丢弃过期结果
     （用户改了查询再回车，旧任务的结果不会覆盖新结果）。进度靠
     `start_progress_ticker` 每 120 ms `cx.notify()` 一次，直到状态不再是
     `Searching` 为止 —— 上游也是定时读 `FolderFindProgress` 的快照。
   - `invalidate()`：取消在跑的任务、`generation += 1`、清结果与折叠/选中状态。
     订阅 `InputEvent::PressEnter` 起搜索，`InputEvent::Change` 且内容与
     `submitted` 不同就 `invalidate` —— 改了查询就立刻撤掉旧结果，
     与上游一致（上游把结果清空但保留搜索框内容）。
   - 结果树：每个文件一行（折叠按钮 + 标签区），标签区可点、可右键
     （*Show in Finder* / *Open in New Tab* / *Copy Path*）；展开后是匹配行，
     命中片段用 `split_display_line()` 切出 head/hit/tail 三段分别着色。
     这个函数把前导空白去掉、把命中前的部分裁到 32 字符并在切点前补 `…`，
     且回退到字节边界 —— 纯函数，所以有 5 项测试盯着它。
   - 末尾一条 `Search Limit Reached`（当 `summary.is_truncated()`），
     文件行内一条 `Only the first N matches were found.`（当 `is_truncated`），
     指标行 `N+ matches in M files`（截断时带 `+`，是下界而不是精确值）。
   - Delete / Backspace 从结果里移除选中的文件（只动 `summary.files`，
     `metrics` 不动 —— 上游如此），所以结果区要 `track_focus`。
   - 两个对话框：`FileScopeSheet`（编辑范围：conjunction 选择 + Name 行 +
     条件行，每行 target / comparison / value + 删除按钮 + *Add Condition* +
     错误行 + Delete / Cancel / Apply）与 `SavedScopesSheet`（已存范围列表，
     行内重命名 / 删除，点击进入编辑）。两者每帧从 pane 重读数据，
     提交后一律 `persist(cx)`。
   - `persist(cx)` 把四个开关 + `folder_find_scope` + `folder_find_saved_scopes`
     写回 `Workspace::settings` 并 `settings.save()`，所以关掉应用再打开，
     范围和已存范围都还在。
   - 10 项测试：5 项盯 `split_display_line()`、1 项盯指标文案、
     1 项盯范围名校验（空名、重名、原名可保留）、2 项是**接缝测试** ——
     `the_panes_options_and_predicate_drive_a_real_search` 与
     `a_scope_narrows_the_walk` 用面板算出的 `Options` 与
     `InclusionPredicate` 去跑一次真实的 `Search`，在自建自删的临时目录上断言结果。
     之所以用这种写法而不是 UI 集成测试，见下面的偏差 6。

3. **接线**：`Settings` 新增 6 个字段（`folder_find_uses_regular_expression` /
   `folder_find_ignores_case` / `folder_find_includes_hidden_files` /
   `folder_find_includes_other_file_types` / `folder_find_scope` /
   `folder_find_saved_scopes`）；`Workspace` 新增 `folder_find: Entity<FolderFindPane>`
   字段，`render_folder_find()` 从占位实现改成渲染这个 entity；
   `Workspace::active_directory()` 给面板提供默认根目录（当前文档所在目录），
   `Workspace::open_and_select(path, range)` 负责「打开文件并选中命中位置」。

**已知偏差（记账）**：

1. **偏移单位**：所有偏移是 **UTF-8 字节**而不是 UTF-16 单元。上游用
   `NSRange`（UTF-16），SubText 的编辑器本身就是字节偏移，两边各自自洽；
   跨层传递时不换算，因为换算需要把整篇文本再走一遍。
2. **编码回退**：用 `DecodedFile` 解码，回退编码（`Detection::Default`）的文件
   直接跳过，另外额外识别**无 BOM** 的 UTF-16 / UTF-32（上游靠 BOM）。
   即：能确定编码的照搜，不能确定的不猜。
3. **文件类型判定**：按**扩展名表**判定而不是 `UTType`。`UTType` 是 Apple 平台
   的机制，跨平台没有对应物。调用方可以传 `InclusionPredicate` 加宽 ——
   面板就是这么做的：它把 `SyntaxManager` 里所有语法的
   `info.file_map.extensions` 收成一个集合，于是「其他文件类型」开关一开，
   所有已知语法覆盖的扩展名都会被搜到，而不是只有硬编码的那几种。
4. **`NSRuleEditor` → 行式规则编辑器**：上游的范围编辑器是 `NSRuleEditor`
   （可嵌套的谓词构建器）。SubText 用「每行一条规则 + *Add Condition*」的平铺列表，
   表达力等价于 Any / All + 单层规则的组合，嵌套结构不支持。
5. **保存 / 管理已存范围**：上游 *Save as Named Scope…* 弹一个输入框、
   *Manage Saved Scopes…* 打开一个管理面板；SubText 把前者做成
   `FileScopeSheet` 里的内联名称字段，把后者做成 `SavedScopesSheet` 对话框。
6. **没有 `test-support`，UI 层不做无头测试**：仓库未启用 gpui-kit 的
   `test-support` feature，也没有 `#[gpui_kit::test]` 用例。与其给面板写一堆
   只能验证「结构没变」的脆弱断言，不如把**可测的部分做成纯函数**
   （`split_display_line` / `metrics_message` / `validate_scope_name`），
   再写两条**接缝测试**：用面板的公开算法算出 `Options` 与谓词，
   喂给真实的 `Search` 跑一遍临时目录。这样测的是「面板 → 核心」这条线，
   而不是「面板长什么样」。
7. **去掉 *Reveal in File Browser***：上游结果行的右键菜单里有这一项，
   依赖一个真正的文件浏览器侧栏。SubText 的 File Browser 只是最近文件列表，
   没有可 reveal 的位置，所以这一项不出现（而不是灰掉）。
8. **不支持跨应用拖拽**：上游可以把结果行拖到别的应用。gpui-kit 的拖放
   只在应用内，跨应用拖拽没有通道。
9. **结果开在本窗口新标签页**：上游每次命中开一个新窗口。SubText 统一走
   `Workspace::open_and_select()`，在现有窗口里新开一个标签页并选中命中范围。
   这是 SubText 全局的导航约定，不只这一处。

### 8.3 多重替换 ✅

上游 *Find ▸ Multiple Replace ▸ Manage Multiple Replace…* 打开一个浮动面板：
左边是命名规则集的侧栏，右边是一张规则表（启用勾选、查找、替换为、`RE`、`IC`、
描述六列），表下是 *Highlight All* / *Replace All* / *In Selection* / 结果行 /
高级选项按钮。

本轮把**规则表本身**完整移植，分两层：

1. **模型**（`subtext-core/src/multireplace.rs`，移植自 `MultipleReplace.swift`
   与 `MultipleReplace+TSV.swift`）：
   - `Replacement` 六字段与上游逐一对应，默认值一致（`is_enabled` 默认 `true`，
     `description` 是 `Option<String>` 而不是空串——上游用 `nil` 区分「没有描述」
     与「空描述」，这个区别决定一条规则算不算「有内容」）；
   - `Settings` 保留上游 *Advanced Find Options* 的全部八个开关。其中四个生效
     （`matchesFullWord` / `isSingleLine` / `isMultiline` / `unescapesReplacementString`），
     四个是**引擎已经做出的决定**（严格比较、忽略变音符号、忽略宽度、Unicode 词边界），
     在面板里渲染为 **disabled + 原因**，不假装可切换；
   - `replace()` 按顺序跑启用规则，每条看到上一条的输出；`find()` 则**每条都搜原文**
     再拼接——这个不对称是上游的，也是两个方法并存的原因；
   - 非法规则（查找串为空、正则不编译）**静默跳过**，`validate()` 是另一个问题，
     它连禁用的规则一起检查；
   - `unescaped_template` 覆盖 Swift 字面量的七种转义。**一处表示不同**：`\\` 在
     这里收敛成单个反斜杠，因为 `regex` 的模板引擎不像 ICU 那样消费反斜杠，
     照抄上游的中间表示会输出两个；
   - TSV 导入：每行取前两个 tab 字段、首字段为空或字段不足即丢弃。
2. **面板**（`crates/subtext-app/src/multiple_replace_sheet.rs`）：
   六列表格（表头 `Find` / `Replace With` / `RE` / `IC` / `Description`，启用列
   留空，与 storyboard 一致）、`+` / `−` 增删、`Advanced` 高级选项、
   *Highlight All* / *Replace All* / *In Selection*、结果行（`N matches` /
   `N replaced`）。每行是独立 entity，行的字段**就是**它的 `InputState`，
   所以输入时只有这一行重绘（警示图标随打随变），整表不动。

**接线**：`"ShowMultipleReplace"` → `Workspace::multiple_replace_dialog()`；
`apply_multiple_replace()` 走 `window.defer` 落文本（保留编辑器自己的撤销历史），
`highlight_multiple_replace()` 把命中范围交给查找栏的同一通道，之后
*Find Next* 可以继续在其中步进。

**已知偏差（记账）**：

1. 上游的侧栏（命名规则集、导入导出、重命名）属于 `ReplacementManager` 的职责，
   本轮不做；因此规则不落盘，只活在面板打开期间。
2. 高级选项在面板内**内联展开**（`Collapsible`）而非 popover：popover 锚在面板内的
   控件上，而面板本身是模态对话框，遮罩会吃掉打开它的那一次点击。
3. 非法规则的说明文字放在表下（红色），上游放在警示图标的 tooltip 里——
   gpui-kit 的 `.tooltip()` 只挂在组件上，普通 `div` 没有。
4. 上游的面板是窗口，靠标题栏关闭；SubText 用对话框，因此补了一个 `Done` 页脚按钮。
5. 面板打开时带一条空规则。上游可以为空（侧栏已经建好了一个定义），SubText 没有
   侧栏，空表读起来像坏了。删掉最后一条会补回一条空规则——这是上游自己的行为。
6. *Highlight All* 只报告命中数并把范围交给查找栏，不绘制高亮——与 SubText 的
   *Find All* 现状一致（编辑器目前没有搜索高亮的绘制通道）。

## 阶段 9 — 验证与发布 v0.2.0 ⏳
| 任务 | 内容 | 状态 |
| --- | --- | --- |
| 9.1 | 全量测试 + clippy 零警告 | ✅（262 项 / 零警告） |
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

