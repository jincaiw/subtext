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
| 7.7 大纲改用 `Tree` | 层级不再用缩进模拟 | P3 | ✅ 见 §8.0 |
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

**8.7d 期间发现的崩溃（已修）**：打开设置窗口会 panic ——
`cannot read subtext_app::workspace::Workspace while it is already being updated`。
根因是开窗路径：Settings… 与 *Edit Snippets…* 都在 `Workspace::update` 里派发，
而 `App::open_window` **在返回前就构造并绘制**新窗口的根视图
（`gpui-pre/src/app.rs:1500-1513`，其中 `window.draw(cx)` 会把 `Render::render` 跑一遍），
于是 `SettingsWindow::new` 的 `workspace.read_with(..)` 与首帧渲染里的
`self.settings(cx)` 都在读一个正被 lease 的实体。改成把整个开窗动作
`cx.defer` 到本轮 effect 周期末尾：`update_entity_erased` 在 `App::update` 的闭包内部
就调了 `end_lease`，`flush_effects` 则在最外层 update 结束时才跑，所以那时 workspace
已经回到实体表里，读它是合法的。开窗本来也不是调用方要等的事，晚一拍没有代价。

这个 bug 从阶段 7.8 重写设置窗口起就存在，一直没被触发过——因为此前所有验证都只
启动了主窗口，从没真正打开过设置窗口。8.7d 的冒烟第一次把它踩了出来。

## 阶段 8 — 补齐高价值缺失功能 ⏳

从 [FEATURE-MATRIX](FEATURE-MATRIX.md) 的 39 项未实现中，按「用户可感知收益 ÷ 工作量」排序：

| 顺序 | 功能 | 理由 | 状态 |
| --- | --- | --- | --- |
| 8.0 | 大纲改 `Tree` | 阶段 7.7 顺延；`Tree` / `TreeState` 已提供 | ✅ |
| 8.1 | 分屏编辑器 | 菜单里 3 项，`h_resizable`/`v_resizable` 已提供 | ✅ |
| 8.2 | 文件夹查找 | 侧栏面板已在，缺查找逻辑 | ✅ |
| 8.3 | 多重替换 | 需要一张可编辑的规则表 | ✅ |
| 8.4 | 颜色代码编辑 | `ColorPicker` 组件已提供 | ✅ |
| 8.5 | 按模式排序 | 纯逻辑，`subtext-core` 可测 | ✅ |
| 8.6 | 选区扩展命令 | 纯逻辑（选包围符号、按行拆分） | ✅ |
| 8.7 | 代码片段 + 文件拖放 | 需要编辑器与存储 | ✅ |
| 8.8 | 快速操作 | 需要面板框架 | ✅ |
| 8.9 | 运行时填充的菜单子菜单 | 8.8 暴露出来的缺口（见下） | 🚧 8.9a ✅ 8.9b ✅ 8.9c ✅ 8.9d ✅ |

**非目标（v0.2 仍不做）**：打印、AppleScript（`Help ▸ AppleScript Dictionary`，
即让别的应用驱动 CotEditor 的 scripting dictionary）、书写方向 RTL、工具栏自定义。
理由：前三者需要平台能力或框架不支持，第四者收益低于成本。Script 菜单里的 `.scpt`
文件**会**列出来并在 macOS 上交给 `osascript` 运行，但脚本里对 CotEditor 自身的
`tell` 无处可去——那是这里说的那个 dictionary。

### 8.0 大纲改用 `Tree` 组件 ✅

阶段 7.7 顺延下来的那条：大纲此前是**自己拿缩进模拟层级**的一列 `ListItem`，
既不能折叠，也没有 CotEditor 的过滤与光标跟随。本轮把它换成 gpui-kit 的
`Tree` / `TreeState`（`uniform_list` 虚拟化），并按上游 `OutlineInspectorView`
补齐整条数据通路。

**上游模型**（`OutlineInspectorView.swift`、`OutlineItem.swift`、
`OutlineExtractor.swift`、`String+Match.swift`）：大纲是一棵 `OutlineItem` 树，
每个条目带一个 `Indent`，而 `Indent` 是**两态**的 —— `.level(n)`（`title` 规则与
`heading.N` 规则）或 `.string(捕获到的空白)`（其余规则）。这个区分不是装饰：
`.string` 会把嵌套层级**清空**并把该条强制为根，所以按缩进排的键值对是一排根
节点，而 Markdown 的 `##` 是 `#` 的子节点。`NSOutlineView` 负责折叠，
`rebuildOutline` 重建、`onChange` 决定哪些新条目**默认展开**，过滤走
`abbreviatedMatchedRanges` —— 是**子序列**匹配而非子串，`gt` 能找到 "Guten Tag"。
内置的 107 条规则里 container 49 / function 33 / value 8 / heading.1–6 各 1 /
heading 2 / title 2 / mark 3 / separator 1 / 无 kind 3。

**三处改动**：

1. **`crates/subtext-app/src/outline.rs`**（1296 行，40 项测试）：从「扁平的
   `level: usize`」重写成「`Indent::{Level, String}` + `kind: Option<OutlineKind>`
   + 稳定 `id`」。新增的全是纯函数，因此都能直接测：

   | 函数 | 对应上游 |
   | --- | --- |
   | `build_tree()` | `OutlineBuildNode.build(from:)`，含 `.string` 清空层级 |
   | `filtered_tree()` | `FilteredItem.filter(_:keyPath:)`，保留通往命中的那条路径 |
   | `expandable_ids()` | `expandableIDs` |
   | `abbreviated_matched_ranges()` | `abbreviatedMatchedRanges(with:)` |
   | `entry_at_line()` | `item(at:)`，取光标所在行**之上最后一个**条目、跳过 separator |
   | `row_map()` / `highlight_segments()` | 绘制用（图标、配色、命中分段） |

   `assign_ids()` 用「kind + 标签」派生身份（上游是存 UUID 再按标签 diff），
   于是重新解析后展开状态还在。

2. **`crates/subtext-app/src/settings.rs`**：`DEFAULT_OUTLINE_FONT_SIZE`（11.0，
   即 macOS 的 `NSFont.smallSystemFontSize`）与 `outline_wraps_lines` /
   `outline_font_size` 两个持久化字段。

3. **`crates/subtext-app/src/workspace.rs`**：`Workspace` 多出一个
   `Entity<TreeState>`、一个过滤框 `Entity<InputState>`、一棵
   `Vec<FilteredNode>`、一份 `HashMap<String, OutlineRow>`（树只把 id 与标签交给
   渲染器，图标与缩进得按 id 反查）、以及展开态镜像 `HashSet<String>`。
   检查器的 body 在大纲页**不再套外层滚动** —— 虚拟化列表自己滚，两层滚动会抢滚轮。

**刷新是怎么被触发的**：上游是「谁改了谁去调 `rebuildOutline`」，SubText 反过来
——渲染时**自查是否过期**。`OutlineSource{document, syntax, 文本哈希, filter}`
记录「当前这棵树是从什么建出来的」，`outline_is_stale()` 拿它和现算的一份比；
`outline_caret_moved()` 比光标所在条目与树当前选中项。任一为真就 `cx.defer_in`
把重建推到效果周期末尾 —— 渲染里不能改 `TreeState`（`set_items` 会 `notify`，
而订阅回调不允许在发射实体的 update 里跑）。于是键入、切标签、从磁盘重读、
换语法、改过滤，五条路径都不必各自记得通知大纲。代价是每次工作区重绘要读一遍
全文（`EditorState::value()` 每次都把整条 rope 物化成 `SharedString`），所以这一遍
读被**共用**给「是否过期」与「光标是否换行」两个判断；而且只在大纲页可见时才发生
——`queue_outline_refresh()` 只从 `render_outline()` 调用。这与文稿检查器现状一致。

**展开策略**（照搬上游 `onChange`）：无过滤时 `retain` 仍存在的 id，并把**新** id
加进展开集（新条目默认展开）；有过滤时整个 `expandable_ids` 展开，否则命中项可能
藏在折叠的父节点里。

**接线**：`TreeEvent::{Expanded, Collapsed}` → 镜像展开态（`set_items` 会清空选中
但不发事件，所以镜像必须自己记）；`cx.observe(tree)` → 行被选中时 `goto_line()`
（上游 `selectItem(id:)`），并用 `outline_selected` 做双向去抖，避免「光标 → 行」
与「行 → 光标」来回弹；过滤框 `InputEvent::Change` → `cx.notify()`。Options 菜单
（`Ellipsis` 按钮 + `DropdownMenu`，锚在右上角）给出 *Text Size* 的
Bigger / Smaller / Reset to Default，字号有下限（不低于默认的 11pt，与上游
`smallerFont` 一致）。

**已知偏差（记账）**：

1. **Options 菜单的 *Wrap Lines* 只显示、不生效**。虚拟化列表的行高必须一致，
   一行折成两行会让 `uniform_list` 的定位算错。菜单项按存下的
   `outline_wraps_lines` 打勾、但禁用 —— 保留是为了让菜单读起来与上游一样，
   同时让那个字段有读取方而不是白存。
2. **点击行内任意位置即折叠/展开**，而非只有折叠控件可点 —— `Tree` 组件的
   `on_entry_click` 行为。折叠控件仍画出来显示状态。
3. **separator 画成整行中间的一根细线**。上游画一个更矮的 `Divider`，但虚拟化
   列表的行高一致，只能把线放进一个整行里。
4. **heading 的图标是 Lucide 近似**。上游用 SF Symbols 的 `N.square.fill`，
   Lucide 没有带编号的方块，所以分级 heading 用 `Hash`，无级别 heading 用
   `SquareMenu`。其余六种 kind 一一对应。
5. **空态保留了 "No outline items"**。上游此时就是一张空列表；SubText 的占位文案
   早于这棵树，留着比空着好读。过滤无结果时用上游的 "No Filter Results"。
6. **光标 → 行同步不会展开祖先**。光标移进折叠节点内部时，那一行没有可选的条目，
   于是不动 —— 上游也不展开。`TreeState::reveal_item()` 能做到，本轮没用，因为
   那会把大纲滚到用户没在看的地方。

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

### 8.7 代码片段（Snippets）✅

上游的代码片段有两条独立的通路，本轮两条都做：

1. ***Text ▸ Insert Snippet* 子菜单**（`SnippetManager.swift` + `Snippet.swift`）：
   用户在设置里维护一张表，每行是「名字 / 可选语法范围 / 可选快捷键 / 插入格式」，
   格式里的 `<<<SELECTION>>>` 与 `<<<CURSOR>>>` 由文档在插入时填。
2. **文件拖放**（`FileDropItem.swift`）：另一张表，每行是「语法 / 扩展名 / 描述 /
   格式」，格式里是 11 个令牌之一，把一个拖进来的文件变成一段文本。

分五块落地：

**8.7a 片段模型**（`subtext-core/src/snippets.rs`，1060 行）
- `Snippet`（`id` / `name` / `scope` / `shortcut` / `format`）与上游字段一一对应；
- `Shortcut` 支持上游的三种拼写（`key-spec` / Cocoa 符号串 / 键盘词），
  `parse` **不做 trim**（上游也不做——多一个空格就是另一个组合）；
- `Rejection` 复刻 `Shortcut+Error.swift` 的四种拒绝，文案逐字一致；
  `Shortcut::rejection()` 的检查顺序也是上游的（Tab/Backtab/Shift-Return →
  `unsupported`；Shift+字母 → `shiftOnlyModifier`；`!is_valid` → `singleType`；
  其余交给调用方查「已被占用」）；
- `Sections` 复刻 `SnippetManager.updateMenu`：无范围片段在前、加分隔线，
  然后按范围分节；**无范围片段的快捷键若与某个范围片段相同则不显示**；
- `Variable` 是 `<<<SELECTION>>>` / `<<<CURSOR>>>` 两个令牌的名字与描述。
- **两处顺序上的坑**（都有测试盯着）：先按行首缩进给格式加缩进、再切分行；
  以及**先切分 `<<<CURSOR>>>`、后替换 `<<<SELECTION>>>`**——反过来的话，
  选中文本里恰好含有的 `<<<CURSOR>>>` 会被当成标记，把光标塞进用户选中的内容中间。
- **不移植**：上游 `insertions(for:ranges:)` 的多选区版本。SubText 的编辑器只有一个
  选区，只移植单选区形式 `Snippet::insertion`。

**8.7b 存储与菜单接线**（`crates/subtext-app/src/snippets.rs`）
片段是**用户数据**，而菜单栏与键位表都在打开任何文档之前就建好了，两者都没法预先
指名一个用户还没写出来的条目。所以 `install()` 每次重算：清空键位表、铺回内置绑定、
再加上片段自己的，最后把带子菜单的整张菜单表设回去。之所以敢这么做，是因为 GPUI
的 keymap 没有「只摘掉某几条」的能力，而两张表都不大（72 条绑定、258 个菜单项）。
- 子菜单只显示适用于当前文档语法的片段，所以 `refresh_snippets()` 在文档打开、
  关闭、切换时都会被调一次——**比较范围相等就直接返回**，因此平时是空操作；
- 设置面板改完片段列表后调 `rebuild_snippets()`，那里范围没动但条目动了；
- 手工编辑过的设置文件仍可能塞进一个与菜单冲突的快捷键，`install()` 会把它丢掉：
  片段绝不能抢走 ⌘S、⌘F 已经在应答的键。

**8.7c 插入到编辑器**（`Workspace::insert_snippet`）
按上游 `insertSnippet(_:)` 的顺序：对选中文本与选区起始行的缩进展开片段、替换选区、
把光标留在 `<<<CURSOR>>>` 处。写回走 `window.defer` + `replace`（不是 `set_value`），
所以 **⌘Z 能像撤销一次按键那样撤销一次片段插入**。

**8.7d 设置窗口的 Snippets 面板**（`crates/subtext-app/src/snippet_pane.rs` + `settings_window.rs`）
复刻 `SnippetsSettingsView.swift`：`TabView` 两个页签（Command / File Drop，
固定高 380pt → 23.75rem），各一张三列表格 + 增删按钮 + 表下高 100pt 的插入格式编辑器。
- 每行是独立 entity，行的字段**就是**它的 `InputState`，所以打字只重绘这一行；
- 格式编辑器由面板持有而非行持有（上游也是「表下单独一个编辑器」），
  只在选中行变化时重新载入，避免打字与回写互相打架；
- `SyntaxPicker` → `Button` + 下拉菜单，首项 `All`；
- `AddRemoveButton` → `+` / `−` 幽灵按钮，**`+` 插到最后一个选中行之后**并选中新项，
  `−` 删掉全部选中项，无选中时禁用；新行名字用上游的 `appendingUniqueNumber`
  （`Untitled` → `Untitled 2` → `Untitled 3`…）；
- `Insert Variable` 菜单插入令牌：Command 页签列 `SELECTION` / `CURSOR`，
  File Drop 页签按 *pathTokens* / *Text File* / *Image File* 三组列 11 个令牌；
- 快捷键录制用**可聚焦 `div` + `capture_key_down`**：`capture` 阶段先于 keymap 匹配，
  所以一个已经被菜单占用的组合（例如 ⌘S）仍能录下来，然后按上游的文案报冲突。
  Esc 取消、Delete 清空、有冲突则回滚并留在录制态——与 `ShortcutField.swift` 一致。

**8.7f 文件拖放**（`subtext-core/src/filedrop.rs` + `imagesize.rs`）
- 11 个令牌、替换顺序、匹配规则（扩展名 + 语法）都与上游一致，所以为 CotEditor 写的
  格式在这里插入同样的文本；
- `<<<FILECONTENT>>>` 与 `<<<IMAGEWIDTH>>>`/`<<<IMAGEHEIGHT>>>` **按需读取**：
  `needs_content()` / `needs_image_size()` 说不需要就不碰磁盘，与上游的懒加载一致；
- 尺寸读取是自己解文件头（PNG / GIF / JPEG / BMP / WebP 五种），因为 SubText 没有
  图像库。读不出来的格式返回 `None`，令牌原样留在文本里——上游对 ImageIO 读不了的
  图片也是这个行为。
- 没有任何条目应答的文件，插入它自己的绝对路径——这正是上游的兜底，也是纯文本
  视图本来会插入的东西。

**已知偏差（记账）**：

1. **「Multiple items selected」文案不可达**。上游在选中多行时把格式编辑器整体禁用并
   提示这句；本实现的表格是**单选**（点一行即换选中），所以只剩「未选中 → No item
   selected」这一种禁用态。要复刻多选得给表格加一个多选模型，收益不抵成本。
2. `ShortcutField` 用 `capture_key_down` 而非 `NSEvent.addLocalMonitorForEvents`。
   两者在「先于 keymap 拿到按键」这一点上等价，但前者是 GPUI 的原生机制，跨平台。
3. 片段与文件拖放列表**存进 `Settings.json`**（`snippets` / `file_drop` 两个字段），
   上游存 `UserDefaults`。字段名与形状对齐到可读的 JSON：上游把扩展名存成一个逗号
   分隔的字符串（那是 `UserDefaults` 字典的遗留），这里存字符串数组，面板里仍按
   逗号分隔的输入框呈现——因为那就是用户敲进去的样子。
4. `<<<IMAGEHEIGHT>>>` 等尺寸令牌只覆盖五种格式；上游借 ImageIO 覆盖系统认识的
   全部格式。未覆盖的格式令牌原样保留。
5. 拖放只处理应用内的 `ExternalPaths`；macOS 的 `.textClipping` 没有对应物，不特判；
   拖进来的网址在 GPUI 里就是一个 `PathBuf`，按路径文本处理而非 URL。

### 8.8 快速操作（Quick Actions / ⌘K）✅

上游的 `Command Bar`（`Panels/Command Bar/`）是一个浮在文档窗口上的无边框面板：
上面一个搜索框，下面列出查询命中的菜单命令，回车执行。SubText 保留**面板**与
**行的内容**，列表本身交给 gpui-kit 的命令面板组件——设计指南要求 command palette
自带选择 / 焦点 / 键盘 / 关闭 / 布局契约，手搓一个盒子等于把这些重做一遍。

分四块落地：

**8.8a 匹配算法进 `subtext-core`**（`subtext-core/src/matcher.rs`）
上游 `String+Match.swift` 的 `abbreviatedMatchedRanges` 是**子序列**匹配（逐字符、大小写
不敏感、只向前推进），不是子串匹配——所以 `tw` 能找到 `Tab Width`。它与大纲过滤用的
是同一套算法，阶段 8.0 已经把这套逻辑写进了 `outline.rs`，本轮把它下沉到 core 共用：
- `abbreviated_match` 同时返回命中区间、**未消费的查询余量**与分数
  （上游 `remaining` / `score` 的对应物）；
- 区间**逐字符不合并**，因为 `remaining` 的算法依赖区间条数
  （`searchString.suffix(count - ranges.count)`），合并了就错；
- 顺手把「只向前推进」写进 doctest：`"Guten Tag"` 查 `gt` 命中的是 `Guten` 里的 `t`，
  不是串里第一个 `t` 之后的那个——这不是 bug，是上游的语义。

**8.8b 生成器产出命令表**（`tools/generate_menu.py` → `menu.rs::COMMANDS`）
上游的命令列表是**运行时遍历 `NSApp.mainMenu`** 得到的，SubText 的菜单表在编译期就
生成了，所以直接把 `COMMANDS`（168 项，`path` + `action`）作为命令来源，与 `SHORTCUTS`
同源同拼写。筛选规则复刻 `ActionCommand.match(command:)`：
- 从路径的**第二段**起匹配（第一段是菜单栏自己的标题，丢掉），加上命令自身标题；
- 第一段必须命中，否则整条跳过；之后每段先吃掉前导空白再匹配，全部匹配完
  **余量必须只剩空白**，否则不算命中；
- 每段分数累加，最后按分数稳定排序（同分保留菜单顺序）。

**8.8c 命令面板视图**（`crates/subtext-app/src/command_bar.rs`）
- 面板是 `WindowKind::Floating` + `titlebar: None` + 透明背景的居中窗口，宽 500pt
  （31.25rem），高度按结果条数算——GPUI 窗口没有内容自适应，而上游的 SwiftUI 面板是
  撑开内容，所以这里用 `window.resize` 自己跟着结果走，上限 300pt（18.75rem）；
- 查询用组件自己的输入框，但**过滤关掉**（`filterable(false)`）：组件的本地过滤是
  单标签的大小写不敏感子串，表达不了「跨菜单路径的子序列」；
- 行内容按上游 `ActionCommandView` 画：类型图标 + 命中路径（命中字符加粗、段间
  chevron）+ 面包屑（未命中的路径段）+ 右侧快捷键提示；
- **空查询不匹配任何东西**——上游就是这样，面板打开时只有搜索框，没有列表；
- Esc 用 `capture_key_down` 在捕获阶段接管（组件的 Cancel 是「先清空非空查询、
  第二次才关闭」，而上游的 Esc 一次就关）；回车与点行都先关窗再执行
  （上游注释：so the action is delivered to the correct (first) responder）。

**8.8d 接线**（`workspace.rs` / `dispatch.rs` / `bridge.rs`）
- `Workspace::show_quick_actions` 开窗，已开则激活而不是再开一个；
- **`bridge` 改为按 workspace 记录它所属的文档窗口**。此前命令一律投给
  `cx.active_window()`，而设置窗口和这个面板都会拿到键盘焦点，那样 ⌘S 会投给面板。
  现在每条命令都由「用户正在编辑的那个窗口」应答。

**两处 GPUI 生命周期上的坑（都有注释盯着，别再踩）**：

1. **开窗必须 defer**。`App::open_window` 在返回前就同步跑一遍根视图的 layout 与 draw，
   而打开本面板的命令来自 `Workspace::update` 内部——此时 workspace 已被移出实体表，
   任何 `read_with` 都会 panic（`cannot read Workspace while it is already being updated`）。
   与设置窗口同一个修法：整段开窗动作包进 `cx.defer`。
2. **行的内容构建器在面板渲染过程中运行，绝不能碰面板实体**。组件的
   `CommandItem::child` 会被测量和绘制各调用一次，而两次都在 `CommandState` 的
   `Render` 之内（`measure_rows` → `render_item_with_binding`）；在实体自己的渲染里读它
   就是双重租借，直接 panic（`cannot read CommandState while it is already being
   updated`）。所以「哪一行被选中」是**以普通 `bool` 传进构建器**的，而不是回头去读面板。
   面板的选中态由 `refresh` 这个 observer 维护成一份副本——observer 跑在面板更新**之后**，
   读它是安全的。

**已知偏差（记账）**：

1. **Script 菜单还是运行时占位项**，所以表里每一条都是菜单命令。上游的第二类
   （脚本）要等 `Script` 菜单填起来才有行可画（见阶段 8.9）。
2. **面板不会在失去焦点时关闭**。上游挂在窗口失焦上；GPUI 没有窗口激活事件，
   所以这里只在 Esc、回车、点行时关闭，点面板后面的文档不会关掉它。
3. 行的**外框**（搜索框字号、行内边距与圆角、列表内缩）用组件族的几何，
   行**内容**按上游逐项对齐到加粗的命中字符。

### 8.9 运行时填充的菜单子菜单 🚧

阶段 8.8 把 `COMMANDS` 当成了完整的命令表，于是暴露出一个更早的缺口：
`menu.json` 里有 **11 个占位项**，它们在上游是**运行时**填出来的，SubText 此前只渲染成
一个置灰的死项：

| 占位项 | 上游的填充来源 | 本轮 |
| --- | --- | --- |
| `Format ▸ Line Endings` | `AppDelegate.prepareMainMenu`，`LineEnding.allCases` | ✅ 8.9a |
| `Format ▸ Syntax` | `AppDelegate.prepareMainMenu`，`SyntaxManager.settingNames` | ✅ 8.9a |
| `Format ▸ Theme` | 同上，`ThemeManager.settingNames` | ✅ 8.9a |
| `Format ▸ Text Encoding` | `AppDelegate.updateEncodingMenu`，`EncodingManager.fileEncodings` | ✅ 8.9c |
| `Text ▸ Unicode Normalizations` | `UnicodeNormalizationForm.standardForms + [nil] + modifiedForms` | ✅ 8.9b |
| `CotEditor ▸ Services` | 系统（`SystemMenuType::Services`） | ✅ 8.9a |
| `Script`（顶层） | `ScriptManager`，`~/Library/Application Scripts` | ✅ 8.9d |
| `Font ▸ Font Type`、`Writing Direction ▸ Orientation` / `Direction` | 上游是 `NSMenuItem.sectionHeader`，本表里已是置灰 `NoOp` 叶 | ✅（即上游形态） |
| `Edit ▸ Insert from iPhone or iPad` | 系统（Handoff） | ✅（即上游形态） |

后果不只是「菜单里少几项」：`set_syntax` / `set_theme` / `set_encoding` /
`set_line_ending` 这些**动作已经实现**，却因为菜单里没有可点的条目而**没有入口**——
只能靠快捷键（如果那条命令有）或设置窗口间接达到。

**8.9a 统一菜单重建通道 + 三个子菜单**（`crates/subtext-app/src/menus.rs`，新文件）

- 新增 `menus.rs` 作为**唯一**的菜单重建入口，`snippets::install` 被它吸收：
  `MenuState`（当前文档的语法/换行符/编码 + 主题 + 语法表 + 主题表 + 片段）
  → `build()` → `cx.set_menus()`。`snippets.rs` 退化成「按语法分好组的片段条目 +
  快捷键绑定」两个纯函数（`items` / `bindings`）。
- **生成器不再渲染置灰叶，而是渲染空子菜单**（`tools/generate_menu.py` 的
  `RUNTIME_SUBMENUS` / `SYSTEM_MENUS`）：`MenuItem::submenu(Menu { items: vec![],
  disabled: true })` 与 `MenuItem::os_submenu("Services", SystemMenuType::Services)`。
  运行时按**名字**就地替换（`replace()` 递归找同名子菜单，找到即置 `disabled: false`）。
  名字在全表内唯一，所以不必把路径写进代码——写路径等于把 storyboard 的结构抄第二遍。
- **参数化动作**：`ChangeLineEnding { ending }` / `ChangeSyntax { name }` /
  `ChangeTheme { name }`。`actions!` 宏只能造 unit struct，带值的要手写 derive
  （与既有的 `InsertSnippet { id }` 同形）。载荷是**菜单上已经显示的那个名字**
  （换行符用缩写 `"CRLF"`，语法/主题用名字本身），`bridge.rs` 把它解析回值——用
  **位置**当载荷的话，列表上方一插入条目就会跑错命令。同时删掉 6 个
  `SetLineEnding*` 与 `SetSyntax` / `SetTheme` 这 8 个占位 unit 动作：上游只有
  `changeLineEnding(_:)` 一个选择器，参数从菜单项里读。
- **勾选态**走 `MenuItem::checked`；**没文档时**语法/换行符子菜单整片置灰
  （上游是 responder 链找不到 target，效果相同）。主题属于应用，无文档时仍可切换。
- **换行符顺序修正**：`LineEnding::ALL` 此前是 `LF, CRLF, CR, …`，而上游
  `allCases` 是 `LF, CR, CRLF, …`——菜单顺序是用户可见的，已改成上游顺序，
  并加了断言。标题也改成上游的 `"{平台} ({缩写})"`（`"Windows (CRLF)"`），
  此前 core 里那个「Line Feed」式的 `description()` 两个菜单都没用过。
- 每次文档打开/关闭/切换、以及 `set_line_ending` / `change_encoding` /
  `reinterpret_encoding` / `set_syntax` / `set_theme` 之后都会重建菜单。
  `refresh_menus` 用 `MenuSignature`（文档 + 主题）挡掉无变化的调用——一次重建是
  全表 258 项的 `App::set_menus`，而每次切标签页都会走这条路。

**已知偏差（记账）**：**NEL / LS / PS 一直列出来**。上游只显示 3 个基本换行符，
另外 3 个按住 Option 才出现（`ControlUI.OptionalMenu` 在菜单打开期间盯着修饰键）。
GPUI 的菜单 API 既没有「隐藏项」也没有修饰键状态，所以 6 个恒列——上游能选的这里
都能选，其中 3 个少按一个键。

**8.9b `Text ▸ Unicode Normalizations`**（`textops.rs` / `menus.rs`）

- core 的 `Normalization` 从 4 种补到上游的 **7 种**，并给出三个常量：
  `ALL`（上游 `CaseIterable` 顺序）、`STANDARD`（前 5）、`MODIFIED`（后 2）——
  菜单的「5 项 + 分隔符 + 2 项」直接由这三个常量铺出来，不是硬编码。
- 每个形态带 `name()`（菜单标签）、`identifier()`（动作载荷）、`from_identifier()`、
  `is_supported()`。
- **后 3 种（NFKC Case-Fold / Modified NFD / Modified NFC）本构建做不了**，如实标注：
  - `nfkcCaseFold` = 去默认可忽略码点 → NFKC → 全量 case fold → NFC → 再去。
    「默认可忽略码点」是 `Default_Ignorable_Code_Point` 属性，「全量 case fold」是
    `CaseFolding.txt`（`ß → ss` 这种，不是 `str::to_lowercase`）。`unicode-normalization`
    0.1.25 **只有 nfd/nfkd/nfc/nfkc**，既没有 case fold 也没有这个属性。
  - `modifiedNFD` 是 `CFStringGetFileSystemRepresentation`，即 Apple 冻结的 HFS+ 表；
    `modifiedNFC` 走 `\P{Full_Composition_Exclusion}` 逐段重合成。两者都不是 Unicode
    规范化形态，是 Apple 自己的表。
  - 处理方式沿用本仓对「菜单项在、能力不在」的既有约定：**菜单照列、动作照发、
    触发时在状态栏说实话**（`Workspace::normalize_unicode` 先问 `is_supported()`），
    而不是悄悄退回一个「差不多」的结果——半规范化的文档用户分辨不出来。
    `textops::normalize` 对这三种形态返回**原样**，文档注释写明这是拒绝不是兜底。
- 动作 `NormalizeUnicode { form }` 取代 `NormalizeNfc/Nfd/Nfkc/Nfkd` 四个 unit 动作。
- 偏差：上游把每个形态的说明挂在菜单项的 `toolTip` 上（`localizedDescription`），
  GPUI 的菜单项没有 tooltip，所以那 7 段说明目前无处可放。

**8.9c `Format ▸ Text Encoding`**（`encoding.rs` / `menus.rs`）

- **注册表每个条目一个显式标识符。** 此前标识符由 codec 派生（`e.name().to_lowercase()`），
  而这个注册表里有多对条目**共用同一个 codec** —— `encoding_rs` 实现的是 WHATWG
  Encoding Standard，不是 Apple 的 codec 集合：`BIG5` 同时是 "Chinese (Big5)" 与
  "Chinese (Big5-HKSCS)"，`GBK` 同时是 "Chinese (GB 2312)" 与 "Chinese (GBK)"，
  `WINDOWS_1252` 同时是 "Western (ISO Latin 1)" 与 "Western (Windows Latin 1)"……
  派生出来的标识符重复，于是菜单项无法区分彼此：两项跑同一条命令，勾选同时落在两项上。
  现在 id 由条目自带，`identifier()` 返回它；`from_identifier()` 先按 id 精确匹配，
  再退回 WHATWG label —— 老设置文件里的拼写仍然解析到**同一个 codec**（不是同一条目，
  重复的那些做不到，也没有必要）。
- **`FileEncoding { encoding, with_utf8_bom }`**，上游的模型。BOM 是**文件**的属性而不是
  UTF-8 的属性，所以它不能做成一条与 codec 并列的注册表条目 —— 那会让注册表声称有两种
  编码而其实只有一种。原先那条 `"Unicode (UTF-8 with BOM)" => UTF_8` 是**死条目**
  （标识符与 UTF-8 相同，`from_identifier` 永远解析不到它），已删除，改由
  `FileEncoding::utf8_with_bom()` 表示。名字按上游的拼法：`"Unicode (UTF-8) with BOM"`
  （上游是 `"\(localizedName) with BOM"`；此前写的是 `"Unicode (UTF-8 with BOM)"`）。
- **`TextEncoding::bom()`**：四种宽 Unicode 形式**带 mark 写出**。此前 `change_encoding`
  一律传 `Bom::None`，选 UTF-16LE 会写出一个字节序无法判定的文件。
- **注册表按上游的分组切。** `MENU_GROUPS` 是表，`ALL` 由它展平（`LazyLock`），两者
  不可能漂移。分组是用户可见的（组间有分隔符），所以照搬上游默认列表的 10 组顺序；
  每组上方注明上游那一组有什么、`encoding_rs` 缺哪几条。
- **子菜单按 `AppDelegate.updateEncodingMenu` 铺**：文档当前编码不在列表里时提到最前 +
  一个分隔符；然后是列表（组间分隔符）；末尾恒为分隔符 + `Customize Encodings List…`。
  "Unicode (UTF-8) with BOM" 紧跟在 UTF-8 之后，因为上游是在
  `EncodingManager.fileEncodings(from:)` 里 `flatMap` 插进去的 —— 这两项永远成对。
- **勾选态是「对」而不是 codec**：带 mark 的 UTF-8 文档勾在 *with BOM* 那一项上，
  与上游 `Document.validateUserInterfaceItem` 比较 `FileEncoding` 一致。`DocumentState`
  与 `MenuSignature` 随之携带 `FileEncoding`。
- 动作 **`ChangeEncoding { identifier }`** 取代 `SetEncoding` / `ReinterpretEncoding`
  两个死占位（storyboard 里没有这两项，它们没有任何入口）。新增 `CustomizeEncodings`
  接上游的 `showEncodingsListEditor`：打开设置窗口的 Format 面板，并在状态栏说明列表是固定的。
- 「当前编码不在列表里则提到最前」这条分支在 SubText **走不到**——列表就是注册表，
  任何文档的编码都在里面。保留它是因为那是上游的行为，且列表一旦可编辑它就得在；
  为此把条目构造拆成 `encoding_items(groups, current)` 纯函数，由测试驱动这条分支。
- 偏差：上游把每个编码的 IANA charset 名等信息挂在菜单项的 `toolTip` 上，GPUI 的菜单项
  没有 tooltip，与 8.9b 同样无处可放。

**已知偏差（记账）**：ISO Latin 1–10 这一组仍是近似 —— `encoding_rs` 没有 ISO 8859-1、
也没有 ISO 8859-9，编码标准把这两个 label 解析到 Windows 代码页，所以 "Western (ISO Latin 1)"
实际以 windows-1252 解、"Turkish (ISO Latin 5)" 实际以 windows-1254 解。两者都是超集，
名字与实现却是两回事；注册表对齐见下方「已知缺口」。

**8.9d `Script` 菜单**（`subtext-core/src/scripts.rs`、`subtext-app/src/scripts.rs`）

- **目录落点**。上游问 `URL(for: .applicationScriptsDirectory, in: .userDomainMask)`，
  即 `~/Library/Application Scripts/<bundle id>`。这个目录由系统为**沙箱应用**建好，
  而 SubText 不是沙箱应用，macOS 之外的平台更没有这个目录，所以 `resources::script_directory()`
  在 macOS 上照抄上游路径（`BUNDLE_ID = "com.subtext.SubText"`，它只用来把一家应用的脚本
  与另一家的分开），其余平台退回 `config_directory()/Scripts`。
- **能力接缝是 `ScriptKind::is_available()`，不是扩展名。** AppleScript 要 Apple event 运行时，
  非 macOS 上为 `false`，`scan` 因此**不把 `.applescript`/`.scpt`/`.scptd` 列进菜单**——
  列出来再点失败，是把平台差异甩给用户。Windows 上还有第二处：它不读 shebang，所以
  `Script::launch()` 按扩展名给解释器（`sh`/`pl`/`php`/`rb`/`py`/`js`/`awk`/`swift`），
  其余平台一律 `Launch::File`，由内核按 shebang 执行。
- **跨平台照搬的部分**：目录扫描规则（跳过 `.` 与 `_` 前缀、按文件名排序、`^\d+\)` 前缀只去前缀
  不去空格、名为 `-` 的文件变分隔符、子目录变子菜单、`.scptd` 先问是不是脚本再问是不是目录）、
  末尾恒为 `[分隔符, "Open Scripts Folder"]`、脚本扩展名里的快捷键（`Clean.@s.sh` → ⌘S）、
  `%%%{CotEditorXInput=…}%%%` / `%%%{CotEditorXOutput=…}%%%` 协议与七种输出目标、
  以及 `applyShortcuts()` 的**语法优先**。
- **`applyShortcuts()` 的优先顺序复刻时踩到过。** 上游先给「与当前文档语法同名的子菜单」里的
  脚本派键，再给其余；但**没有**这个同名子菜单时用的是 `recursively: false`，只先派顶层那批，
  子菜单里的脚本并不因此获得优先。第一版实现写成了「各子菜单各自保留同一键」，
  被 `the_folder_named_after_the_syntax_gets_the_keys_first` 揪出来。
- **`NSFilePresenter` 换成轮询。** 上游盯目录变化重算菜单，GPUI 没有对应的文件观察者。
  `Workspace` 每 2 秒（`SCRIPT_POLL`）`scripts::scan()` 一次，与 `MenuSignature` 比对，
  不同才重建——用户往文件夹里丢一个脚本，菜单两秒内自己长出来。探针实测
  `step 0: 1 scripts` → `step 1: 2 scripts`。
- **脚本在主线程起、进程在后台跑、结果回来再落。** `run_script` 先查脚本列表（命令按路径找脚本，
  路径必须对着同一份列表解析）、读源码、解 token、取输入（选区 / 当前行 / 全文）与文档路径，
  再把进程交给后台执行器——`execute` 会阻塞到子进程退出，界面线程不能等——回来后由
  `finish_script` 落结果。`OutputType::range` 给出七种目标各自的区间：`Pasteboard` 走剪贴板、
  `NewDocument` 建新文档（**空输出不建**，否则一个什么也没干的脚本会留下一个空标签页），
  其余在 `window.defer` 里 `set_selected_range` + `replace` + 把光标停在插入文本之后——
  与代码片段同一条理由，`replace` 会发 `Change`。
- **偏差：没有修饰键行为。** 上游普通点击运行脚本、Option 点击在编辑器里打开、Option+Shift
  在 Finder 里显示。GPUI 的菜单项不带修饰键状态，所以点击**恒为运行**。脚本文件夹本身与
  *Open Scripts Folder* 两条入口不受影响。
- **偏差：没有 tooltip。** 上游给每个脚本项挂 `"Option-click to open script in editor."`，
  GPUI 的菜单项没有 tooltip，与 8.9b、8.9c 同样无处可放。
- **偏差：`tell application "CotEditor"` 无处可去。** AppleScript 在 macOS 上交给 `osascript`
  跑，但上游的示例脚本里有靠 CotEditor 自己的 scripting dictionary 驱动的，那需要 SubText
  反过来暴露一个 dictionary，不在本阶段范围内。
- **未实现：事件脚本。** 上游的 `CotEditorHandlers`（`documentOpened` / `documentSaved`）
  让脚本在文档打开或保存时自动跑。GPUI 侧没有对应钩子，本阶段不做。
- **比上游多说一句**：脚本非零退出但 stderr 为空时，上游看着像成功了（它只看 stderr），
  SubText 在状态栏说明失败。**另有一条实测得来的约束**：两个 token 必须写在脚本的**注释**里
  ——裸写一行 `%%%{CotEditorXInput=AllText}%%%` 会被 `/bin/sh` 当命令执行并报
  `fg: no job control`。上游的示例脚本本来就是注释形式，但文档里没写这件事。
- `Open Scripts Folder` 会先 `create_dir_all`：上游是沙箱应用，系统已把目录建好；SubText 不是，
  非 macOS 上这个目录根本不存在，而它又是唯一能把脚本放进菜单的入口。

**8.9e 让 `COMMANDS` 包含运行时条目**：`Syntax ▸ Swift`、`Theme ▸ Solarized Dark`
这类条目上游在 Quick Actions 里也能搜到，本表目前搜不到——`COMMANDS` 只由
`menu.json` 生成。要把运行时的「子菜单名 ▸ 条目名」在构建时并进去。

## 阶段 9 — 验证与发布 v0.2.0 ⏳
| 任务 | 内容 | 状态 |
| --- | --- | --- |
| 9.1 | 全量测试 + clippy 零警告 | ✅（385 项 / 零警告） |
| 9.2 | 冒烟运行 + 打包 | ⏳ |
| 9.3 | 更新 CHANGELOG、README、FEATURE-MATRIX | ⏳ |
| 9.4 | 提交、打 `v0.2.0` 标签、重新打包 | ⏳ |

---

## 已知缺口（不阻塞，但需记账）

| 缺口 | 说明 |
| --- | --- |
| `cargo fmt` 未接入 | 全仓 20+ 个文件与 rustfmt 默认风格不一致（`actions!` 等宏调用的换行方式无法用 `rustfmt.toml` 对齐）。`menu.rs` / `keymap.rs` 是生成文件，要接入 fmt 得先让 `tools/generate_menu.py` 输出 rustfmt-clean 的代码（`actions.rs` 只被生成器**读取**，不写入）。当前门禁是 **clippy 零警告 + 测试全绿**，不含 fmt。 |
| Unicode 规范化的 3 个 Apple 形态 | `NFKC Case-Fold` / `Modified NFD` / `Modified NFC` 需要 `Default_Ignorable_Code_Point`、全量 case fold（`CaseFolding.txt`）与 Apple 的 HFS+ 表，`unicode-normalization` 0.1.25 都没有。菜单照列、触发时在状态栏说明（见 §8.9b）。 |
| 编码注册表与上游默认列表尚未逐条对齐 | `encoding_rs` 实现的是 WHATWG Encoding Standard（一个以 Windows 为中心的**子集**），不是 Apple 的 codec 集合。上游 `DefaultSettings+Encodings.swift` 的默认菜单列表有 60 条，SubText 只能提供其中约 40 条：Mac OS 系列（MacJapanese、MacChineseTrad/Simp、MacKorean、macArabic、macGreek、macHebrew、macCyrillic、macCentralEurRoman、macTurkish、macIcelandic）、Big5-E、EUC-TW、DOS 系列（DOSJapanese、DOSChineseTrad、DOSLatinUS、DOSRussian）、NextStep Latin、ASCII 都没有对应 codec；反过来 SubText 多出四条上游默认列表里没有的（KOI8-R、KOI8-U、Vietnamese Windows、Baltic Windows）。注册表里还有几对**共用 codec 的条目**（GBK / GB 2312、Big5 / Big5-HKSCS、Windows Latin 2 / Central European (Windows)、Turkish 两条、Baltic 两条、Shift JIS / Shift JIS X0213），名字与标识符都不同、各自可解析，但选任意一条效果相同。**本轮（8.9c）只修了会被菜单直接暴露的错**：ISO Latin 5–10 此前映射到完全不同的字母表（Latin 5 → ISO-8859-5 西里尔、6 → ISO-8859-6 阿拉伯、7 → ISO-8859-7 希腊、8 → ISO-8859-8 希伯来、9 → ISO-8859-10、10 → ISO-8859-13），现在按上游的名字与 Apple `CFStringEncodings` 的对应关系改成 windows-1254 / ISO-8859-10 / ISO-8859-13 / ISO-8859-14 / ISO-8859-15 / ISO-8859-16，Latin 2 与 4 的名字也对齐成上游的 "Central European (ISO Latin 2/4)"。剩下的一次性对齐见任务 #37。 |
| `Script` 菜单的四处偏差 | 无修饰键行为（上游 Option 打开 / Option+Shift 显示，GPUI 菜单项不带修饰键状态，点击恒为运行）；无 tooltip（上游的 `"Option-click to open script in editor."`）；`tell application "CotEditor"` 无处可去（SubText 不暴露 scripting dictionary）；事件脚本（`CotEditorHandlers` 的 `documentOpened` / `documentSaved`）未实现。目录变化由 2 秒轮询代替 `NSFilePresenter`。详见 §8.9d。 |
| 默认编码设置没有 BOM 选项 | 上游的默认编码是一个 `FileEncoding`，配一个 `saveUTF8BOM`（"Save UTF-8 files with BOM"）。SubText 的 `Settings::default_encoding` 只存一个 codec 标识符，所以设置窗口的 "Default text encoding" 选择器里没有 *Unicode (UTF-8) with BOM* —— 菜单里有，设置里没有。 |
| Windows / Linux 未验证 | 只验证过 `aarch64-apple-darwin`。 |
| 冒烟测试的非 GUI 环境崩溃 | 10-14 23:34 有一份 `did_finish_launching` 的 SIGABRT 报告，发生在由 agent shell 以后台角色启动时。3 次复现未出现，未定位。 |

