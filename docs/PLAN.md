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

## 阶段 8 — 补齐高价值缺失功能 ✅

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
| 8.9 | 运行时填充的菜单子菜单 | 8.8 暴露出来的缺口（见下） | ✅ 8.9a–8.9h |
| 8.10 | 大纲导航命令 | 8.0 留下的大纲条目只有行号，没有位置 | ✅ |
| 8.11 | 编码优先级列表编辑 | 上游探测顺序就是这份列表；SubText 用的是统计猜测 | ✅ |
| 8.12 | 桌面 UI 与布局按设计规范复核 | 两个技能的规范是**要求**；逐条对账而非凭印象 | ✅ |
| 8.13 | 侧栏与检查器可拖动改宽 | 上游是 `NSSplitViewController`，两条分隔线都能拖且会自存 | ✅ |
| 8.14 | 默认编码是一个「文件编码」 | 上游选的是 `FileEncoding`（codec + BOM）；SubText 选 codec，BOM 开关是死的 | ✅ |
| 8.15 | 文档窗口的状态栏与导航栏 | 上游两者都不在窗口这一层：状态栏属于内容列、导航栏属于每个分屏 | ✅ |
| 8.16 | 文档检查器按上游重排 | 8.15 读窗口层次时发现检查器的行集是自编的；顺带查出三个计数（Characters / Lines / Words）算错 | ✅ |
| 8.17 | Inspect Character 改成锚定选区的 popover | 8.16 补 `ScalarInfo` 时发现：上游是 popover（`preferredEdge: .minY` + `.semitransient`），SubText 是模态对话框、十行平铺 | ✅ |
| 8.18 | 自定义包围 + 五个内置包围的语义 | 文本菜单最后一个占位提示；做它的时候用上游 fixture 发现内置命令一直包的是**行**而不是**选区** | ✅ |
| 8.19 | 高级计数重做成「选项 + 胶囊」 | 上游是带七个选项的活计数器（sheet → 胶囊 → 齿轮 popover），SubText 是一个静态对话框 | ✅ |
| 8.21 | 警告检查器按上游重排成两栏 | 上游是 `VSplitView` 的两个面板（不兼容字符 / 不一致换行符），各带状态行与表格；SubText 是一列自编的提示。顺带查出保存路径丢掉了有损标志，会把 `&#65533;` 静默写进文件 | ✅ |
| 8.20 | 补全 + 三个被 base64 吃掉的快捷键 | `Edit ▸ Complete` 是占位提示，Mode 面板的三个补全开关没人读；顺带发现 ⌘⌫ / ⇧⌘⌫ / ⌥⎋ 三条键位在 storyboard 里是 base64 存的，一直没被读出来 | ✅ |

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

### 8.9 运行时填充的菜单子菜单 ✅

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

8.9g 又查出**第 12 个**运行时填充点：`File ▸ Open Recent`。它不在上表里，因为它的占位形态
**不是空的**——storyboard 已经给了它一个 `Clear Menu` 子项，看着像做完了，见 §8.9g。

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
  Encoding Standard，不是 Apple 的 codec 集合：`BIG5` 同时是 "Traditional Chinese (Big 5)"
  与 "Traditional Chinese (Big 5 HKSCS)"，`SHIFT_JIS` 同时是两条日文 Shift JIS，
  `WINDOWS_1252` 同时是 "Western (ISO Latin 1)" 与 "Western (Windows Latin 1)"……
  派生出来的标识符重复，于是菜单项无法区分彼此：两项跑同一条命令，勾选同时落在两项上。
  现在 id 由条目自带，`identifier()` 返回它；`from_identifier()` 先按 id 精确匹配，
  再退回 WHATWG label —— 老设置文件里的拼写仍然解析到**同一个 codec**（不是同一条目，
  重复的那些做不到，也没有必要）。条目名与分组的逐条对齐见 §8.9f。
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

**当时的偏差（已被 §8.9f 收掉）**：ISO Latin 1–10 这一组当时仍是近似 —— `encoding_rs` 没有
ISO 8859-1、也没有 ISO 8859-9，编码标准把这两个 label 解析到 Windows 代码页，所以
"Western (ISO Latin 1)" 实际以 windows-1252 解、"Turkish (ISO Latin 5)" 实际以 windows-1254 解。
两者都是超集，名字与实现却是两回事。§8.9f 把整张表与上游默认列表逐条对齐时确认了这仍是
**唯一可行的选择**（实现一张自己的 ISO 8859-1/9 表会让可编码字符集变窄），于是保留并记账。

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

**8.9e 让 Quick Actions 看到运行时条目**（`menus.rs` / `command_bar.rs` / `bridge.rs`）

- **上游的列表不是表，是菜单栏。** `CommandBarWindowController.showWindow` 在面板出现前
  取 `NSApp.actionCommands`，那是把**实时主菜单**摊平：子菜单递归进去、把父标题插到每条
  子孙路径前面，叶子只在 `isEnabled && !isHidden` 时入选，另有两个 selector 被列在
  `unsupportedActions` 里排除。所以上游能搜到 `Format ▸ Syntax ▸ Swift`——那条目在那时
  才存在。SubText 此前搜的是 `menu::COMMANDS`，一张编译期从 storyboard 导出的表，
  永远不可能有它。
- **改为读菜单栏**：`menus::commands(&state)` 把 `build(state)` 的结果按上游的规则摊平。
  面板打开时取一次（`show_quick_actions` 里 `menus::commands(&self.menu_state())`），
  与上游在 `showWindow` 里取一次同理。探针实测 316 条，其中 168 条来自表、其余是运行时的。
- **三条上游的入选规则都复刻了**：子菜单标题进路径；`disabled` 的叶子不入选
  （所以没有文档时 `Format ▸ Syntax`、`Line Endings`、`Text Encoding` 一条都不在，
  而 `Customize Encodings List…` 在——它不是改编码的入口；snippet 的分组标题是
  `disabled` 的，作为标签也不入选）；以及 **`View ▸ Show Quick Actions…` 被排除**——
  在搜索框里搜打开搜索框的命令是个环，上游正是为此把它列进 `unsupportedActions`。
  顺带修掉一处既有偏差：此前那条命令是能被 ⌘K 搜到的。
- **`Workspace::perform` 按动作运行，不按名字。** 运行时条目的动作携带它代表的值
  （语法名、主题名、编码标识符、规范化形式、snippet id、脚本路径），没有名字可查。
  `bridge::perform` 是唯一同时懂两套词汇的地方，`Workspace::perform` 先问它，
  不是它才退回按名字分发（名字取 `Action::name()` 的最后一段，即 `menu.json` 里那一个）。
  `bridge.rs` 里原先七个手写的 `cx.on_action` 收成 `bind_runtime_actions!` 一张表 +
  `perform` 一个函数，菜单点击与面板走同一条路。
- **为什么不是 `window.dispatch_action`。** 那会把动作交给全局 handler，而
  `bridge::run` 从注册表里挑 workspace（最旧的先赢）——面板是从哪个窗口开的就是哪个窗口，
  多窗口时两者不是一回事。上游 `ActionCommand.perform()` 走的是**该窗口**的响应链，
  所以命令必须留在它来的那个窗口里，这正是面板已经持有 `WeakEntity<Workspace>` 的原因。
- **两种字形。** 上游命令画 `filemenu.and.selection`、脚本画 AppleScript 字形；
  组件库里最近的一对是 `SquareMenu` 与 `SquareTerminal`，同一个方框，两行文字起点对齐。
  `CommandKind` 由动作是不是 `RunScript` 决定。
- **快捷键统一到上游的写法。** 生成的 `keymap::SHORTCUTS` 第 2 列把和弦写成 `"⌘S"`、
  Control 写成 `⌃`；而上游 `Shortcut.symbol`（`Packages/MacUI/Sources/Shortcut/Shortcut.swift`）
  是 `(modifierSymbols + [keyEquivalentSymbol]).joined(separator: .thinSpace)`，且
  `ModifierKey.symbol` 的 Control 是 `"^"`。也就是说 `Shortcut::symbol()` 才是忠实的，
  那一列不是。同一个列表不能有两种拼法，所以每一行都过 `Shortcut::symbol()`。
  **不读那一列的显示形式**：它把键一律写成大写（菜单就是这么显示的），
  所以大写字母说明不了 Shift，读回来会凭空多一个 ⇧——`Shortcut::parse("⌘S")` 就是这个坑。
  改成读**修饰键列 + 键列**（`row_shortcut`），两者无歧义。
- **偏差（记账）**：面板的宽度、行高、列表内边距仍是组件族的（设计指南要求面板如此），
  行内容照上游，粗体命中位置一致。上游在面板失去 key window 时关闭，GPUI 没有对应的
  窗口激活事件，仍只按 Esc / Enter / 点行关闭。
- **`menu::COMMANDS` 的角色变了。** 它不再是命令的来源，只剩编译期记录 + 测试用
  （`the_live_bar_agrees_with_the_generated_table` 把它和实时菜单栏逐条对起来）。
  生成器里那段「GPUI 读不回菜单，所以面板读这张表」的说明已经过时——面板不是去**读**
  已安装的菜单，而是把同一根菜单栏**重建**一遍再走，不需要平台支持。生成器的注释已改，
  `menu.rs` 重新生成（`keymap.rs` 未变，两次生成 sha256 一致）。

**8.9f 编码注册表与上游默认列表逐条对齐**（`encoding.rs` / `menus.rs` / `settings_window.rs`）

8.9c 只修了会被菜单直接暴露的错（ISO Latin 5–10 的 codec、Latin 2/4 的名字）。这一节把
上游 `DefaultSettings+Encodings.swift` 的默认列表**逐条**过一遍，改掉能改的、合并掉重复的、
把该如实记账的记清楚。

**先把名字的事实拿准，再动手。** 上游那份列表里每一条都挂着一条注释，看起来就是
`String.localizedName(of:)` 的结果；但注释是手抄的，第 109 行把 `UTF32LE` 写成了
「Unicode (UTF-16LE)」。所以没有照抄注释，而是用系统 SDK 写了个小程序把 60 条的
`CFStringConvertEncodingToIANACharSetName` / `localizedNameOfStringEncoding` **实测**了一遍。
实测推翻了两处：

- `GB_18030_2000` 的注释写「Chinese (GB18030)」，Foundation 实际是 **「Chinese (GB 18030)」**
  （带空格）——SubText 原来的名字本来就是对的，不动。
- 反过来，`ISO_2022_JP` 是「Japanese (ISO 2022-JP)」（无连字符）、`EUC_JP` 是
  「Japanese (EUC)」、`EUC_KR` 是「Korean (EUC)」、`dosThai` 是「Thai (Windows, DOS)」、
  `KOI8_U` 是「Ukrainian (KOI8-U)」（不是 Cyrillic）、`windowsLatin5` 是
  「Turkish (Windows Latin 5)」——这些原来都写错了。

**60 条的去向：38 条进菜单，22 条如实缺席，另有 6 条从菜单降级、1 条补进降级层。**

| 上游组 | 上游条目 | 处置 | 说明 |
| --- | --- | --- | --- |
| Unicode | Unicode (UTF-8) | ✅ 照搬 | |
| Japanese | Japanese (Shift JIS) | ≈ 近似 | codec 是 cp932（Windows-31J），JIS X 0208 Shift JIS 的超集 |
| Japanese | Japanese (EUC) | 🔤 改名 | 原「Japanese (EUC-JP)」 |
| Japanese | Japanese (Windows, DOS) | ❌ 缺席 | cp932，就是上一条的 codec；同一 codec 第二个名字只加菜单项、不加能力 |
| Japanese | Japanese (Shift JIS X0213) | ≈ 近似 | 同上 codec，没有 JIS X 0213 plane 2 |
| Japanese | Japanese (Mac OS) | ❌ 缺席 | `encoding_rs` 没有任何 Mac 日文 codec |
| Japanese | Japanese (ISO 2022-JP) | 🔤 改名 | 原「Japanese (ISO-2022-JP)」 |
| Western | Western (Mac OS Roman) | ✅ 照搬 | |
| Western | Western (Windows Latin 1) | ✅ 照搬 | |
| Chinese | Chinese (GB 18030) | ✅ 照搬 | 注释写成 GB18030，实测带空格，SubText 原本就对 |
| Chinese | Traditional Chinese (Big 5 HKSCS) | 🔤 改名 + ≈ | 原「Chinese (Big5-HKSCS)」；WHATWG Big 5 表含 HKSCS，但映射到真实码点而非 PUA |
| Chinese | Traditional Chinese (Big 5-E) | ❌ 缺席 | Big 5-E 的扩展也在 WHATWG Big 5 表里，同上 |
| Chinese | Traditional Chinese (Big 5) | 🔤 改名 | 原「Chinese (Big5)」 |
| Chinese | Traditional Chinese (Mac OS) | ❌ 缺席 | 无 Mac 中文 codec |
| Chinese | Simplified Chinese (Mac OS) | ❌ 缺席 | 同上 |
| Chinese | Traditional Chinese (EUC) | ❌ 缺席 | 无 EUC-TW codec |
| Chinese | Simplified Chinese (GB 2312) | 🔤 改名 + ≈ | 原「Chinese (GB 2312)」；codec 是 GBK（超集） |
| Chinese | Traditional Chinese (Windows, DOS) | ❌ 缺席 | cp950，就是 Big 5 那条的 codec |
| Chinese | Simplified Chinese (Windows, DOS) | ❌ 缺席 | cp936，就是 GB 2312 那条的 codec |
| Korean | Korean (Mac OS) | ❌ 缺席 | 无 Mac 韩文 codec |
| Korean | Korean (EUC) | 🔤 改名 + ≈ | 原「Korean (EUC-KR)」；codec 实为 cp949 |
| Korean | Korean (Windows, DOS) | ❌ 缺席 | cp949，就是上一条的 codec |
| Thai | Thai (Windows, DOS) | 🔤 改名 | 原「Thai (Windows)」 |
| Thai | Thai (ISO 8859-11) | ≈ 近似 | codec 是 windows-874，只差 `0x80`–`0x9F` |
| Arabic | Arabic (Mac OS) | ❌ 缺席 | 无 Mac 阿拉伯文 codec |
| Arabic | Arabic (ISO 8859-6) | ✅ 照搬 | |
| Arabic | Arabic (Windows) | ✅ 照搬 | |
| Greek | Greek (Mac OS) | ❌ 缺席 | 无 Mac 希腊文 codec |
| Greek | Greek (ISO 8859-7) | ✅ 照搬 | |
| Greek | Greek (Windows) | ✅ 照搬 | |
| Hebrew | Hebrew (Mac OS) | ❌ 缺席 | 无 Mac 希伯来文 codec |
| Hebrew | Hebrew (ISO 8859-8) | ✅ 照搬 | |
| Hebrew | Hebrew (Windows) | ✅ 照搬 | |
| Cyrillic | Cyrillic (Mac OS) | ❌ 缺席 | **见下方「Mac Cyrillic 的坑」** |
| Cyrillic | Cyrillic (ISO 8859-5) | ✅ 照搬 | |
| Cyrillic | Cyrillic (Windows) | ✅ 照搬 | |
| Cyrillic | Russian (DOS) | ➕ 新补 | cp866，`encoding_rs` 有 `IBM866`，此前漏了 |
| 杂项组 | Central European (Mac OS) | ❌ 缺席 | 无 Mac 中欧 codec |
| 杂项组 | Turkish (Mac OS) | ❌ 缺席 | 无 Mac 土耳其文 codec |
| 杂项组 | Icelandic (Mac OS) | ❌ 缺席 | 无 Mac 冰岛文 codec |
| ISO Latin | Western (ISO Latin 1) | ≈ 近似 | 编码标准把 label 解析到 windows-1252 |
| ISO Latin | Central European (ISO Latin 2) | ✅ 照搬 | |
| ISO Latin | Western (ISO Latin 3) | ✅ 照搬 | |
| ISO Latin | Central European (ISO Latin 4) | ✅ 照搬 | |
| ISO Latin | Turkish (ISO Latin 5) | ≈ 近似 | label 解析到 windows-1254 |
| ISO Latin | Nordic (ISO Latin 6) | ✅ 照搬 | |
| ISO Latin | Baltic (ISO Latin 7) | ✅ 照搬 | |
| ISO Latin | Celtic (ISO Latin 8) | ✅ 照搬 | |
| ISO Latin | Western (ISO Latin 9) | ✅ 照搬 | |
| ISO Latin | Romanian (ISO Latin 10) | ✅ 照搬 | |
| 中欧 Windows | Latin-US (DOS) | ❌ 缺席 | cp437，`encoding_rs` 没有 |
| 中欧 Windows | Central European (Windows Latin 2) | 🔤 改名 + 合并 | 原「Western (Windows Latin 2)」与「Central European (Windows)」两条同 codec，合并成这一条 |
| 中欧 Windows | Western (NextStep) | ❌ 缺席 | NextStep Latin 无 codec |
| 中欧 Windows | Western (ASCII) | ❌ 缺席 | 无 ASCII codec；`encoding_rs` 的 `ascii` label 解析到 windows-1252，那会接受 ASCII 该拒绝的字节 |
| Unicode | Unicode (UTF-16) | ❌ 缺席 | 见下方「缺的两条宽 Unicode」 |
| Unicode | Unicode (UTF-16BE) | 🔤 改名 | 原「Unicode (UTF-16 Big-Endian)」 |
| Unicode | Unicode (UTF-16LE) | 🔤 改名 | 原「Unicode (UTF-16 Little-Endian)」 |
| Unicode | Unicode (UTF-32) | ❌ 缺席 | 同 UTF-16 |
| Unicode | Unicode (UTF-32BE) | 🔤 改名 | 原「Unicode (UTF-32 Big-Endian)」 |
| Unicode | Unicode (UTF-32LE) | 🔤 改名 | 原「Unicode (UTF-32 Little-Endian)」 |

**Mac Cyrillic 的坑（一条值得单独记的发现）。** 上游列的是 `macCyrillic`，Foundation 给它的
IANA 名就是 `x-mac-cyrillic`，而 `encoding_rs` 的 `X_MAC_CYRILLIC` 正带着这个名字——看起来是
完美对应。但 `encoding_rs` 自己的文档写着「This is the MacUkrainian encoding from Mac OS
Classic... This encoding matches the Windows code page 10017」，而 Apple 的 `macCyrillic`
是 cp10007、`macUkrainian` 是 cp10017（Foundation 名「Cyrillic (Mac OS Ukrainian)」）。
也就是说 WHATWG 的 `x-mac-cyrillic` 用的是**乌克兰文**那张表，却挂着西里尔文的名字。
把这条 codec 命名为「Cyrillic (Mac OS)」会犯「名字说一张表、解码用另一张表」的错，
所以：菜单里**如实缺席**，codec 以 Foundation 给它的真名「Cyrillic (Mac OS Ukrainian)」
放进降级层。这是实测 + 读 crate 文档才发现的，不是能从上游源码推出来的。

**缺的两条宽 Unicode。** 上游的 Unicode 组是 6 条：`UTF-16`、两个字节序、`UTF-32`、两个字节序。
`UTF-16` / `UTF-32` 的意思是「让 BOM 决定字节序」。SubText 的 `FileEncoding` 恰恰相反——
它把字节序做成显式的（`utf-16le` / `utf-16be`），`Bom` 是**文件**的属性。要支持
「未指定字节序」就得先决定写出时用哪个字节序，而上游是从 Foundation 继承的（跟平台原生
字节序走，不跨平台可复现）。所以这两条如实缺席，留给「`FileEncoding` 支持未指定字节序」
这个独立缺口，不在这里猜一个定义。

**降级层 `UNLISTED`：SubText 认得、但菜单不列。** 上游的编码列表是
`String.availableStringEncodings` 的**子集**（设置面板可以从全集里加，默认列表是精选的 60 条）。
SubText 之前把这两层混成一层，于是菜单里多了 6 条上游默认列表没有的。现在拆开：

- `MENU_GROUPS` = 菜单，38 条，与上游默认列表能覆盖的部分逐条对齐；
- `UNLISTED` = 7 条，`AVAILABLE` = 两者相接 = 45 条；
- `from_identifier` 搜 `AVAILABLE`，菜单 / 探测器 / 设置选择器一律用 `MENU`——
  与上游一致（它的 `fileEncodingCandidates` 和编码列表编辑器都基于列表本身）。

降级的 6 条：`Cyrillic (KOI8-R)`、`Ukrainian (KOI8-U)`、`Turkish (Windows Latin 5)`、
`Baltic (Windows)`、`Vietnamese (Windows)`、`Chinese (GBK)`；第 7 条是上面那个
`Cyrillic (Mac OS Ukrainian)`。**不删它们**的理由有两条：老设置文件里的拼写还能解析
（`koi8-r` 这些是 `id`），以及缺口表里那个「列表编辑器」将来要有东西可加。

**合并掉 4 对重复条目。** 上游把 windows-1250 只列一次，SubText 列了两次；ISO 8859-2、
8859-4、8859-9 也各有一条在两组里各出现一次。合并后：`windows-latin-2` 与
`iso-8859-2` / `iso-8859-4` / `iso-8859-9` 这四个旧 id 仍要能解析，前三个是 WHATWG label
（回落就能找到同 codec 的存活条目），只有 `windows-latin-2` 不是 label，进了
`LEGACY_IDENTIFIERS` 别名表。有一个测试专门盯这四个。

**顺带修掉一处自己引入的回退。** 设置窗口的「默认编码」选择器此前用 `ALL`（现在叫 `MENU`）
填选项，降级之后如果用户的设置里存着 `koi8-r`，选择器会显示空白（文档仍能按它解码）。
按编码菜单既有的「当前值不在列表则提到最前」规则补上：不在列表时把当前值插到选项最前。

**验收方式是把对账写成测试。** `UPSTREAM_DEFAULT_LIST`（60 条）与 `UPSTREAM_LISTED`（38 条）
两张常量表进测试模块，断言：60 = 38 + 22、`MENU` 的名字序列**恰好**等于 `UPSTREAM_LISTED`、
`UPSTREAM_LISTED` 是 `UPSTREAM_DEFAULT_LIST` 的子序列、22 条缺席的**不在** `AVAILABLE` 里、
降级层恰好是那 7 条 id。于是任何一次改名、增删、或「把缺席的悄悄加回来」都会让测试红。
命名与上游不一致这件事从「靠人记得」变成了「编译期+运行期都拦得住」。

**残余近似（记账）**：38 条里有 8 条是超集近似 —— ISO Latin 1 → windows-1252、
ISO Latin 5 → windows-1254、Thai (ISO 8859-11) → windows-874、两条 Shift JIS → cp932、
GB 2312 → GBK、Korean (EUC) → cp949、Big 5 HKSCS → WHATWG Big 5（HKSCS 映射到真实码点）。
都是「解码不会漏、只会多接受几个字节序列」的方向，名字与实现不一致这点在缺口表里。

**8.9g File ▸ Open Recent 的最近文档列表**（`menus.rs` / `bridge.rs` / `workspace.rs` / `actions.rs`）

上面那张表里没有这一条，因为它不属于「空子菜单」：`menu.json` 里 `File ▸ Open Recent`
**有**一个子项——storyboard 写下的 `Clear Menu`。它看着是完整的，所以这轮做「死菜单命令」
审计时才发现真相：上游的 `Open Recent` 子菜单在 storyboard 里标着
`systemMenu="recentDocuments"`，那是 AppKit 的钩子，`NSDocumentController` 会在
`Clear Menu` **之上**逐条插入最近文档。SubText 既没有这个钩子、也没有任何运行时填充，
于是这个子菜单永远只有一项。

审计的产物还有两个**死动作**：`actions::file::OpenRecent` 与 `actions::help::ReleaseNotes`
——声明了动作类型、写了 bridge 映射、写了 dispatch 臂，**却没有任何菜单项、快捷键或命令表
条目能触发它们**。查上游：`openRecent:` / `releaseNotes:` 这两个 selector **不存在**
（Help ▸ Release Notes 用的是 `openHelpAnchor:`，File ▸ Open Recent 是个容器），
所以它们不是「上游有、SubText 缺」，而是凭空声明的。已删除。

顺带把 FEATURE-MATRIX 里两个手抄的数字换成实测的：上游 storyboard 有 **170 个
`<action>` 元素、去重后 155 个 selector**（原记 153），`actions.rs` 里 **176 个类型化动作**
（168 个单元动作 + 8 个带载荷的动作；原记 182）。`actions.rs` 的模块注释也一并改了口径，
并写明「一个 selector 可以对应多个动作、也可以有一个动作不被任何 selector 触及，
但不能有一个动作没人能触发」——这正是这轮审计的判据。

实现：

- 新动作 `file::OpenRecentDocument { path }`，与 `RunScript` / `ChangeLineEnding` 同形：
  GPUI 的菜单项带的是动作而不是 target + tag，所以路径随动作走。用**路径**而不是位置——
  打开一个文档就会重排整个列表，按位置跑会开错文件。
- `menus.rs` 新增 `OPEN_RECENT_MENU` 与 `recent_documents(state)`：最近文档（最多
  `RECENT_DOCUMENT_LIMIT` 条）+ 分隔符 + `Clear Menu`。分隔符只在上面有内容时才加，
  与上游同形（空历史时子菜单就是 `Clear Menu` 一项，没有横线压在上面）。
- `MenuState` / `MenuSignature` 都带上这份列表，于是「打开文档」
  （`open_path` → `note_recent` → `push_document` → `refresh_menus`）与
  「Clear Menu」（`clear_recent` 现在也调 `refresh_menus`）都会重建菜单栏，
  而其余时刻被签名比较挡掉。
- `bridge::perform` 增一条分支（按路径 `open_path`），`bind_runtime_actions!` 增该动作。

两处细节是上游的行为，都是**实测**得来的，不是从源码推的：

| 行为 | 实测方式 | 结果 |
| --- | --- | --- |
| 菜单最多列 10 条 | `clang -framework Cocoa` 探针读 `NSDocumentController.maximumRecentDocumentCount` | `10`；CotEditor 不覆盖它。设置文件仍存 20 条（侧栏是可滚动的列表，菜单不是） |
| 没有最近文档时 `Clear Menu` 置灰 | 同一探针调 `validateUserInterfaceItem:`，自造一个实现 `NSValidatedUserInterfaceItem` 的对象当菜单项 | 空列表时 `clearRecentDocuments:` 返回 `NO`，有文档时返回 `YES` |

置灰这件事有第二个后果，也是上游的：Quick Actions 的列表来自**实时菜单栏**的遍历，
而遍历跳过禁用项——所以空历史时 `File > Open Recent > Clear Menu` 不在 ⌘K 里。
SubText 的 `menus::commands` 里 `walk` 跳过 `disabled` 正是同一条规则。
`the_live_bar_agrees_with_the_generated_table` 因此要给状态一个最近文档才成立
（storyboard 无条件写下 `Clear Menu`，而它在实时菜单里出现与否是个**状态**），
空历史那一侧由 `an_empty_history_leaves_just_the_clear_item_and_greys_it_out` 从反面盯着。

**条目标题用的是文件名**，侧栏的最近文件行用的也是 `file_name()`。AppKit 在那里写什么是
它自己的渲染，这个构建读不到；「最近文档」的名字就是文件名，而路径是菜单放不下的第二行。
这点如实记在缺口表里，没有假装与 AppKit 一致。

验收：6 条新测试（历史在前 + 分隔符 + `Clear Menu` 收尾；空历史只剩置灰的 `Clear Menu`
且不在 ⌘K 里；14 条只列 10 条；每条携带自己的路径；无文件名的路径按路径列出；
签名跟着列表走）。全工作区 **504** 项测试全绿（498 → 504），clippy 零自有警告，
生成器幂等（`menu.rs` `d72a9524` / `keymap.rs` `75c5c9e0`，与 8.9f 时一致），
冒烟：开两个文档存活 5 秒、stderr 0 字节、`Settings.json` 里 `recent_documents` 按最新优先
记录了这两条。

**8.9h `Edit ▸ Substitutions ▸ Straighten Quotes`**（`subtext-core/src/textops.rs` / `dispatch.rs` / `workspace.rs`）

8.9g 的审计顺手把 Substitutions 组这四条重新过了一遍，结论是它们不是一类东西：

| 菜单项 | 上游实现 | 结论 |
| --- | --- | --- |
| `Straighten Quotes` | `straightenQuotesInSelection:` 是 CotEditor **自己的**（`EditorTextView+Transformation.swift` → `String.straighteningQuotes`） | 纯变换，**可照搬** |
| `Replace Quotes` / `Replace Dashes` / `Replace Text` | 三个 selector 都不在 AppKit 公开头文件里；探针实测「`NSTextView` 实例响应、类不响应」 | `NSTextView` 的私有实现（替换面板的系统服务），⚠️ 平台受限 |

所以这一轮把 `Straighten Quotes` 实现掉，另外三条的措辞从「尚未实现」改成「系统替换服务仅
macOS 可用」——与拼写/语音那一组同口径。

实现：

- `subtext-core/src/textops.rs` 新增 `straighten_quotes(text, selection) -> Edit`，与同模块的
  `transform_case` / `transform_width` / `normalize` 同形：只改选区，返回替换后的缓冲区与
  应当跟随的选区，空选区是不做任何事的 `Edit::unchanged`。
- 映射严格照上游那两行：`U+2018..=U+201B`（‘ ’ ‚ ‛）→ `'`，`U+201C..=U+201F`（“ ” „ ‟）→ `"`。
  这**不是** toggle：`«»`、`‹›`、`′″`、全角 `＂` 与直的 `'` / `"` 都不在类里（有测试盯着）。
- `dispatch.rs` 把原来四条共用的一个臂拆开：`StraightenQuotesInSelection` 走真实现，
  另外三条给平台受限的说明。

**一处已知偏差**：上游 `validateUserInterfaceItem` 里 `straightenQuotesInSelection:` 有一条
显式校验——`isEditable && !selectedRange.isEmpty`，注释还写明理由：「虽然它在空选区下也能
工作（会作用在光标所在的词上），仍然禁用它，好让它的状态与 `replaceQuotesInSelection:` 一致」。
GPUI 的菜单项只有一个固定的 `disabled` 标志，没有「按当前选区重新问一次」的钩子；要让这一项
跟着光标动，就得在每次选区变化时重建整条 258 项的菜单栏，这个代价不值得。所以这一项**保持可用**，
空选区时在状态栏说 `Select something first`（与 `Use Selection for Find` 同口径），
**两种情况都不改文本**——可观察到的结果与上游一致。这条记在缺口表里。

验收：4 条新测试（八个码点各归位；`«»`/`‹›`/`′″`/全角/直引号一律不动；只改选区且多字节
偏移正确；空选区是 no-op）。全工作区 **508** 项测试全绿（504 → 508），clippy 零自有警告。

### 8.10 大纲导航命令 ✅

`Find ▸ Select Previous Outline Item`（⌃⌘↑）与 `Select Next Outline Item`（⌃⌘↓）此前是两条
占位臂，状态栏回一句「Outline navigation is not implemented yet」。

上游语义（`OutlineNavigator.swift` 加 `OutlineItem.swift` 的 `BidirectionalCollection` 扩展）：

| 步骤 | 上游 |
| --- | --- |
| 「光标所在的那个条目」 | `indexOfItem(at:)` = `lastIndex { $0.range.location <= location && !$0.isSeparator }` |
| 上一个 | `previousItem(for:)`：先取 `indexOfItem(at: range.lowerBound)`，再在它**之前**找最后一个非分隔项 |
| 下一个 | `nextItem(for:)`：若选区**末端**在第一个条目之前，答案就是第一个条目；否则取 `indexOfItem(at: range.upperBound)` 之后第一个非分隔项 |
| 落点 | `textView.select(range: item.range)` = 设选区 + `scrollRangeToVisible` + 让文本视图成为第一响应者 |

关键的一点是**判据是范围不是行**：`item.range` 是正则**整个匹配**的范围（`OutlineExtractor` 里的
`result.range`），所以像 SVG 的 `id="…"` 那样匹配在行中间的规则，落点也在行中间。

于是这一轮做的其实是「**条目知道自己站在哪**」。8.0 起 `OutlineEntry` 只有 `line`，凡是要位置的地方
都拿行当替身；`range` 一旦有了，四处替身一起换掉：

| 位置 | 之前 | 现在 |
| --- | --- | --- |
| `sort_key` | `(line, indent.level())`，注释写明「用层级代替匹配长度」 | `(range.start, range.len())`，即上游的 `(location, length)` |
| 光标落在哪个条目 | `entry_at_line(entries, line_of_offset(text, caret))` | `entry_at(entries, caret)` |
| `extract` 的去重 | `a.line == b.line && a.title == b.title` | `a.range == b.range && a.title == b.title` |
| 点大纲行 | `goto_line(entry.line)` | `set_selection(entry.range)` |

前两处不换会**互相矛盾**：规则匹配在行中间时，行会认领光标而范围不认，于是大纲高亮第 k 行、
⌃⌘↑ 却从第 k+1 行起步。第 3、4 处是同一个「行是范围的替身」的顺带修正——上游 `selectItem(id:)`
选的就是 `item.range`，点 `fn main` 应该选中 `fn main`，而不是把光标丢到那一行行首。

`line` 字段因此整个删掉：它唯一的**生产**读取就是去重（其余是测试），而它的算法是每命中一次就
扫一遍前缀，代价是 O(命中数 × 文本长度)，每次按键刷新大纲都要付。删掉之后 `OutlineEntry` 与上游
`OutlineItem` 的字段对齐（`id`/`title`/`range`/`indent`/`kind`——上游没有行）。`line_of_offset`
随之无人调用，一并删除（含它的两条测试）。

实现：

- `outline.rs`：新增 `previous_item` / `next_item` / `index_of_item` 三个纯函数，逐例照搬上游；
  `entry_at` 换掉 `entry_at_line`。
- `workspace.rs`：`select_previous_outline_item` / `select_next_outline_item` 走同一个
  `step_outline_item(forward)`——读活动缓冲区与选区、按 `document.syntax` 取该语法的大纲规则、
  重新抽取一次、找条目、把选区设到 `entry.range` 上。
  **重新抽取而不是读 `outline_entries`**：后者只在大纲面板渲染时刷新，而上游是从语法控制器读
  `outlineItems`，面板关着也照样工作。代价是每次按键解析一遍缓冲区，但这是用户手按出来的动作。
- `dispatch.rs`：两条命令从占位臂改成实际调用。

**一处偏差**：上游找不到条目时菜单项是**置灰**的；GPUI 的菜单项只有固定 `disabled`，要跟着光标动
就得每次选区变化重建整条菜单栏（与 §8.9h 同一个理由）。所以保持可用，状态栏说
`No previous outline item` / `No next outline item`，选区不动、文本不动。

验收：**8 条新测试**——上游 `OutlineCollectionTests` 的 `previousItem()` / `nextItem()` 逐例照搬
（含 `NSRange(40..<61)` 这种跨两个条目的选区）、分隔符在两个方向上都不是答案、多字节条目按字节
偏移、同跨度同标签才合并、同行不同跨度是两个条目；删掉 2 条随 `line_of_offset` 一起走的测试。
全工作区 **514** 项测试全绿（508 → 514），clippy 零自有警告。生成器幂等
（`menu.rs` `d72a9524` / `keymap.rs` `75c5c9e0`，与 8.9f 起一致——这一轮没有碰生成输入）。
死菜单命令探针重跑：165 条菜单命令，**0 条** `dispatch` 不认。
冒烟：开一份含大纲的 Markdown 存活 5 秒、stderr 0 字节。

### 8.11 编码优先级列表编辑 ✅

`Format ▸ Text Encoding ▸ Customize Encodings List…` 与设置窗口 Format 面板的
*Edit List…* 此前一条是占位臂、一条是置灰按钮，两处都说「列表是固定的」。这一轮把
上游的 `EncodingListView` 整个搬过来。

#### 先要弄清上游到底怎么定编码

`about_autodetect_encoding.html` 写的是四步，`String+Encoding.swift` 的
`String.string(data:options:)` 是它的实现，顺序为：

| 步骤 | 条件 |
| --- | --- |
| ① BOM | 且 `options.candidates` 含 `bom.candidateEncodings` 里的那个 |
| ② xattr | `com.apple.TextEncoding`；SubText 不读 |
| ③ 声明 | `options.respectsDeclaration` 且 `candidates` 含声明里那个编码 |
| ④ 列表 | **按 `candidates` 顺序逐个试，取第一个能解出且没有替换字符的** |
| ⑤ 兜底 | 都不行就抛错，由调用方用默认编码 |

关键在④：**上游没有统计探测**。`candidates` 就是用户那份优先级列表，所谓「自动识别」
是「按用户排的顺序试」。SubText 此前是「BOM → UTF-8 有效性 → `chardetng` 统计猜测 →
默认编码」——与上游**根本不同**：`chardetng` 会把一份 Big5 文件猜成 GBK，而上游只会
按用户列表里 GBK 与 Big5 的先后给答案。所以 `chardetng` 依赖整个删掉（core 与
workspace 各一行），`Detection` 的两个变体 `Utf8Valid` / `Guessed` 换成
`Declared` / `Candidate`。

#### `subtext-core`：`EncodingList`

`EncodingList(Vec<Option<TextEncoding>>)`，`None` 是分隔符。上游 `EncodingItem` 用
`kCFStringEncodingInvalidId` 当分隔符，且 `separator` 是**每次访问都新造一个**——
说明分隔符没有身份，只有位置。照搬的方法：

| 上游 | SubText |
| --- | --- |
| `move(fromOffsets:toOffset:)` | `move_up` / `move_down`（返回落点，供 UI 同步行序） |
| `canRemove(ids:)` | `can_remove(index, default_encoding) -> Option<RemovalError>` |
| `remove(ids:)` | `remove(index)`（不自己问 `can_remove`，与上游一致） |
| `restore()` | `restore_defaults()` |
| `save()` | `Serialize`（写成 `Vec<Option<String>>`） |
| `addSeparator(after:)` / `addEncoding(_:after:)` | `insert_after(selection, item) -> usize` |
| `canRestore` | `is_default()` |

**一处设计决定**：`from_identifiers` 对「解析不出的标识符」是**丢弃**而不是变成分隔符。
变成分隔符会让菜单凭空多出空行，且与「用户自己放的分隔符」混淆——上游的 `nil` 只有
一个来源（用户放的），不该被两种含义共用。`a_list_that_resolves_to_nothing_falls_back_to_the_defaults`
与 `an_encoding_this_build_cannot_read_is_dropped` 两条测试盯住这里。

`FileEncoding::menu_groups()` 由无参改成收 `&EncodingList`：按分隔符**切**成组再重新
拼。上游是逐项映射（`fileEncodings(from:)` 一次过），两者对任何列表给出同一序列；
`the_separators_survive_the_cut_and_join` 用 6 种刁钻列表（前导 / 尾随 / 连续分隔符）
证明这一点。

#### `subtext-core`：声明扫描器

`scan_declaration()` 只读**前 1024 字节**（上游 `scanEncodingDeclaration` 的
`NSRange(0..<1024)`），用 `.isoLatin1` 解——不可能失败，且每字节一字符，偏移即文件偏移。
三条规则照搬上游：CSS `^@charset "…";`、注释 `(?:(?:file)?encoding|coding)[:=] *["']? *…`
（**只看前两行**）、HTML `\scharset\s*= *["']…["']`（`.ignoresCase()`，且**匹配点之前
必须全是 ASCII**——否则会在正文里认领 `charset=`）。三者取 `range.lowerBound` 最早的
那个，然后 `from_identifier` 认领；认不出的当没看见。

这里有一个真实的坐标错位 bug 被测试逮到：注释式规则是**按行**匹配的，正则给出的偏移是
相对**行首**的，直接当文件偏移用会让第二行的 `# coding:` 显得比第一行的 `@charset` 更早。
修法是 `first_lines(text, count)` 迭代器同时产出 `(offset, line)`，匹配后加回 `offset`。

#### `subtext-app`：设置、菜单、探测

`Settings` 新增 `encoding_list: EncodingList`（上游的 `encodingList`，`#[serde(default)]`
所以旧设置文件照读）。`Settings::detection_candidates()` 给出 `Vec<TextEncoding>`，
`detection_options(&candidates)` 把它与 `refer_to_encoding_tag` 配成 `DetectionOptions`。
`MenuState` 新增 `encoding_list`，`menus::encodings(state)` 改从它建子菜单。`workspace`
的 `open_path` / `revert` 走新探测；`Document::open` / `revert` 因此多一个
`&DetectionOptions` 参数，`folderfind` 传 `DetectionOptions::defaults()`（它没有设置可读）。
`Workspace::encodings()` 那个「把 `'static` 的 `MENU` 交给调用方」的方法删掉——它正是
「列表固定」这个假设的化身。

设置窗口的 **Default text encoding** 选择器也改从用户列表取（上游用的是
`encodingManager.fileEncodings`，同一份）；列表里没有当前编码时仍把当前编码插到最前，
这样一份早先版本写下的设置文件不会让选择器空着。

#### `subtext-app`：编辑器

`encoding_list_sheet.rs`，1117 行（含 18 条测试）。上游是 SwiftUI 的 `List` + `.sheet`，SubText 是
设置窗口上的一个对话框——上游的 *Customize Encodings List…* 也是「打开 Format 面板，
再请面板呈现它自己的 sheet」，只是它靠 `FormatSettingsView.Presentation` 这个请求对象
跨窗口传话；SubText 两扇窗之间是一次调用，请求对象就不需要了。调用本身要等窗口：`open_settings`
是 `cx.defer` 出去的，所以 `show_encoding_list_editor` 把「请它呈现」也 defer 在同一队列
之后（`App::flush_effects` 是 `VecDeque` 从前取，`defer` 从后放，顺序有保证）。

**编辑的是副本**：不按 OK 什么都不会写进设置，所以 Cancel 与 Escape 都是零成本。
*Restore Defaults* 也是拿它做判据——与上游的 `canRestore` 同一个比较。

**编辑方法全是纯方法**（不接 `cx`），只改 `list` / `rows` / `selection` / `anchor` /
`addable`，由各 click 处理器在调用后调一次 `cx.notify()`。这是为了让全部**数据规则**
（拒绝、插入点、移动的形状）都能在没有窗口的环境里测。`rows` 与 `list` 是两个平行结构，
`assert_in_step()` 是它们的不变量：debug 构建下每次编辑后都断言，测试里也一样。
行的身份来自一个计数器，不能是行号（移动会改），也不能来自编码（分隔符无从派生，手改的
设置文件可以让同一个编码出现两次）。

#### 与上游的偏差（都记账）

| 偏差 | 说明 |
| --- | --- |
| 拖拽排序 → 箭头按钮 | `gpui-component` 0.7.3 没有拖拽重排，也没有能上报 Delete 键的列表（`grep -r on_drag src/` 为空）。能力相同——任何条目都能一次挪一格——手势不同。 |
| Delete 键 → Delete 按钮 | 同上。上游的按钮本来就由 `canRemove` 置灰，SubText 的也是；上游那条**没有**被置灰拦住的是 `.onDeleteCommand`，SubText 没有那条路。 |
| 列表上方那句话 | 上游是 *Drag encodings to change the order*，SubText 是 *Move encodings up or down to change the order*。用户跟不动的指示比换一种说法的指示更糟。 |
| 选择器里没有分隔线 | 上游的选择器把分隔符画成 `Divider()`。SubText 的 `pick_list` 是各面板共用的一列按钮，加分隔线要动那个共用件，不在这一轮。 |
| 对话框标题 | 上游的 sheet 没有标题栏（`presentationSizing(.fitted)`）。GPUI 的 `Dialog` 需要一个，用 *Encoding List*——上游这张表的表名。 |
| 拒绝消息取哪一条 | 选中的既是 UTF-8 又是默认编码时，消息属于**列表里靠上**的那条（上游 `canRemove(ids:)` 是 `items.filter(with: ids)` 取第一个，不跟选区）。已照此实现，`the_first_refusal_in_list_order_is_the_one_reported` 盯住。 |
| UTF-8 那一行画两行 | 上游 `EncodingView` 对 `.utf8` 画「Unicode (UTF-8)」与「Unicode (UTF-8) with BOM」两条，右侧 IANA 名两条都是 `utf-8`。已照此。 |

#### 顺带修掉的一个真 bug：三个副窗口没有 Base `Root`

冒烟探针（临时钩子派发 `CustomizeEncodings`）第一次跑就崩了：

```
panicked at gpui-component-0.7.3/src/root.rs:112:45:
component window state is missing; call gpui_component::init before gpui_kit::open_window
```

原因：`gpui_kit::open_window` 会把 builder 返回的视图包一层 `base::Root`，而
`cx.open_window`（GPUI 自己的）不会。主窗口走的是前者，**设置窗口、关于窗口、快速操作面板
走的都是后者**——所以那三扇窗从来没有组件窗口状态，也就从来没有能力打开任何 overlay。
在 §8.11 之前没人发现，是因为那三扇窗此前都不开对话框：设置窗口的 *Edit List…* 是置灰的，
关于窗口只画字，快速操作面板自绘。

这一轮把三处都改成 `gpui_kit::open_window`。随之而来的一个连带改动：窗口的根视图变成
`Root` 之后，`handle.downcast::<SettingsWindow>()` 不再成立，所以 `Workspace` 多存一个
`settings_view: Option<WeakEntity<SettingsWindow>>`——窗口句柄回答「窗还在不在」，
弱实体回答「还有没有人可以说话」，两者一起清。这也是 `bridge` 对主窗口已经在做的事
（句柄与内容实体分开存）。

**这条值得单独记**：它是「框架的初始化步骤不能省」的一个实例，而且是**静默**的——省掉
`Root` 不会在启动时报错，只会在第一次用到 overlay 时崩。判断办法很简单：
`grep -n 'cx.open_window' crates/` 应该为空。

验收：**40 条新测试**——core 里「优先级列表」11 条 + 「声明」6 条 + 「流水线」5 条，
app 里编辑器 18 条（含两条方向按钮的启用判据）。全工作区 **554** 项测试全绿
（514 → 554），clippy 零自有警告。生成器幂等（`menu.rs` `d72a9524` / `keymap.rs` `75c5c9e0`，
与 8.9f 起一致——这一轮没有碰生成输入）。死菜单命令探针重跑：165 条菜单命令，
**0 条** `dispatch` 不认。
冒烟（临时钩子，提交前已移除，`git diff | grep -c PROBE` = 0）：4 个命令各跑一遍
（`ShowAboutPanel` / `ShowQuickActions` / `ShowSettingsWindow` / `CustomizeEncodings`）
存活 10 秒、stderr 只有探针自己的两行；重复派发同一命令（走「窗口已开」那条分支）
同样存活。

### 8.12 桌面 UI 与布局按设计规范复核 ✅

`gpui-kit` 与 `gpui-kit-design-guides` 两份规范是**要求**，不是参考。这一轮拿它们逐条对账，
只看**能被判据抓住**的东西：规范点名了某个助手、某条规则，就在代码里查它有没有被用对。
结论是布局骨架是对的（壳、列、滚动所有者都有声明），但有一批「能用但没按规范写」的地方，
和**一处算得出来的溢出**。

#### 一处真溢出：查找栏在最小窗口下装不下

窗口最小宽度是 720 px（`lib.rs` / `open_settings` 各写一次）。查找栏最宽时有 9 个子项：
两个**定宽** `w(rems(14.))` 输入框、计数、四个命令按钮、关闭按钮。按默认 16 px 基准字号算：

| 项 | 宽 |
| --- | --- |
| 两个输入框 | 448 px |
| 四个文本按钮 + 关闭 | ~288 px |
| `px_3` 内边距 + 八处 `gap_2` | ~40 px |
| **合计** | **~776 px** |

超出 720 px 约 56 px，而两个输入框定宽、计数那格没有 `truncate()`，所以窄窗下**尾部按钮被裁掉
且无法到达**。规范「Responsive desktop windows」那一条正是问这个：窗口变窄时会发生什么。

改法照规范的两条：**输入框改成有下限的弹性宽度**（`flex_1().min_w(rems(8.)).max_w(rems(20.))`，
既能在窄窗里缩到 8 rem，又不会在宽窗里无限拉长），**计数格加 `min_w_0().truncate()`**
（一条长错误信息现在截断，而不是把 *Replace All* 推出边界）。主任务（输入查询、逐个跳转）
在窗口允许的每个宽度上都在。

#### 规范点名、而代码没用对的地方

| 规范 | 原文 | 之前 | 现在 |
| --- | --- | --- | --- |
| 布局 · 弹性子项 | 「apply `min_w_0()` or `min_h_0()` to the flexible child that is allowed to shrink」 | 13 处写 `min_w(px(0.))` | 全部改 `min_w_0()` |
| 布局 · 行列不对称 | 「Give a full-height column `h_full()`, or give the row `items_start()` / `items_stretch()`」 | 中栏 `h_flex()` 靠三个子列各自 `h_full()` 撑着 | 中栏显式 `.items_stretch()`，子项漏写也不会被裁 |
| 布局 · 滚动所有者 | 「Put content inset inside that scroll owner rather than wrapping the scroll owner in a padded container」 | 设置窗口主面板 `p_4` 与 `overflow_y_scroll` 同一元素；`pick_list`、编码列表同理 | 三处都拆成「外框负责滚动、内层负责内边距」，滚动条贴区域边缘而不是穿在内边距里 |
| 主题 · 令牌优先 | 「Application UI should not contain raw hex, `rgb`/`rgba`, or `hsla` colors」 | 大纲七种 kind 的颜色散在 `icon_color()` 的分支里；匹配高亮的前景是行内字面量 | 收进 `outline::symbol` 常量模块（七个 `NSColor.system*`）与 `MATCH_FOREGROUND`，两处都写明**为什么不是主题角色** |
| 间距 · rem 优先 | 「application layout should use GPUI's rem-based scale helpers」 | 大纲缩进 `pl(px(depth * 12.))`、展开三角槽 `w(px(12.))`、文件夹查找的勾选槽 `w(px(12.))` | 全部改 rem（`OUTLINE_INDENT = 0.75`），三处共用同一格宽 |

**剩下的每一处 `px(` 现在都有理由，且理由写在旁边**：窗口最小尺寸（OS 约束）、
`command_bar::window_height`（`window.resize` 要 `Pixels`）、两条一物理像素的分隔线、
编辑器与大纲的字号（用户设置的数据，不是界面缩放）。应用层已无「裸的」间距或颜色。

#### 对账过但**不改**的（避免为规范而规范）

- 侧栏与检查器的 `h_flex()` 子列都有 `h_full()`，所以 `items_center` 的裁切陷阱本来就没咬到——
  加 `items_stretch()` 是把意图写明，不是修 bug。
- 设置窗口的 `field()` 标签列是**右对齐定宽**，这是上游 `Grid` 的
  `gridColumnAlignment(.trailing)`，不是 SubText 自创的；规范要求「标签与控件对齐」，
  右对齐满足它。
- `primary()` 七处全部是「决策区的默认提交」（Go / Apply / Rename / Sort / Replace All /
  Insert / 编码列表 OK），没有一处是「因为它唯一」或「因为它靠右」。
- 文案已合规：无标签以句号结尾、无三点省略号；打开对话框的命令带 `…`（菜单表里上游本来就带）。

#### 记账的新缺口

侧栏与检查器**不能拖动改宽**：它们有 `min_w` / `w` / `max_w` 三条约束（规范要求的
「最小 / 舒适默认 / 如何吃掉富余」），但没有拖拽手柄。上游是 `NSSplitViewController`，
可以拖。改成 `h_resizable` 需要把中栏做成可调整组，而编辑器区分屏自己已经是一层
`h_resizable`——嵌套之后的视觉结果这一轮无法验证（没有 UI 集成测试，见缺口表），
所以按本仓惯例**如实记账而不猜**。→ **§8.13 已补上，且是量出来的，不是猜的。**

验收：无新增测试（这一轮改的是布局与令牌，判据是编译、clippy、算术与冒烟）。
全工作区 **554** 项测试仍全绿、clippy 零自有警告、生成器幂等、
冒烟开一份 Rust 文件存活 7 秒且 stdout/stderr 各 0 字节。

### 8.13 侧栏与检查器可拖动改宽 ✅

§8.12 记账的那条缺口。上游 `WindowContentViewController` 是一个
`NSSplitViewController`，三个 item（sidebar / content / inspector）的两条分隔线都能拖，
而且 `splitView.autosaveName` 会把位置存下来；它的 inspector item 只给了
`minimumThickness = 200`，sidebar item 的三条约束来自视图自己的约束。

SubText 的中栏从「三个定宽列」改成 `h_resizable` 组：

| 面板 | `size` | `size_range` | flex |
| --- | --- | --- | --- |
| 侧栏 | `sidebar_width`（默认 15 rem） | 11–24 rem | `flex_none()` |
| 编辑器 | 无 | 无 | 默认（吃掉富余） |
| 检查器 | `inspector_width`（默认 16 rem） | 12–26 rem | `flex_none()` |

三个新常量成对放在 `settings.rs`（`SIDEBAR_MIN_WIDTH` / `_DEFAULT_` / `_MAX_WIDTH`），
`WindowOptions` 新增 `sidebar_width` / `inspector_width` 两个字段持久化，
拖动结束时 `on_resize` 把解析出的像素宽度换算回 rem 写回设置（写回前 clamp）。

#### 一个必须先解决的问题：面板宽度是按位置存的

`ResizableState` 按**组 id + 面板位置**存尺寸。四种显隐组合若共用一个 id，
隐藏侧栏时编辑器会挪到 slot 0，把侧栏的宽度继承过来 —— 下次开窗文档区就是 15 rem。
所以组 id 带上显隐状态（`workspace-columns-{sidebar}{inspector}`），每种组合各有各的尺寸，
且都从设置里的宽度起算。

#### 验证：不是「看起来对」，是量出来的

没有 UI 集成测试，但这不代表只能猜 —— `ResizableState::sizes()` 给出**解析后的像素宽度**。
临时用一个 `with_state` 持有状态 + 一个 `SUBTEXT_SMOKE_SPLIT` 钩子每 0.6 秒打印一次，
四种情形各测一遍（探针提交前已移除，`grep -c PROBE` = 0）：

| 情形 | 期望 | 实测 |
| --- | --- | --- |
| 默认宽度，两栏都开（窗口 1536 px） | 15 rem / 富余 / 16 rem | `[240, 1040, 256]` ✅ |
| 存了 20 rem / 22 rem | 320 / 富余 / 352 | `[320, 864, 352]` ✅ |
| 隐藏侧栏（存的是 20 rem） | 编辑器**不得**继承 320 | `[1184, 352]` ✅ |
| 三帧连续采样 | 不抖动 | 三次相同 ✅ |

最后一行才是 §8.12 那条缺口的答案：宽度**是**可以量出来的，
所以「无法验证」这个理由不成立 —— 该做的是造一个量具，不是记一笔账。

#### 顺带逮到的一个真 bug（比这个功能重要）

探针第一次跑只打印出一个面板 `[1536]`，说明**侧栏根本没显示** ——
写进去的 `{"window":{"show_sidebar":true}}` 没有被读出来。原因不在探针：

`Settings` 上有 `#[serde(default)]`，但**嵌套的六个 `*Options` 结构都没有**。
容器级的 `serde(default)` 只覆盖「顶层键缺失」；一个**存在但不完整**的嵌套对象会让整个
`Settings` 反序列化失败，而 `Settings::load()` 是 `unwrap_or_default()` ——
于是**用户设过的每一项设置被静默丢弃**，而那份文件按设计留在磁盘上，用户永远不知道为什么。

这条对**向前兼容**同样致命：给任何一个嵌套结构加字段（比如这一轮的 `sidebar_width`），
磁盘上已有的设置文件就会全部读不出来。也就是说这一轮自己就踩了这个坑。

修法是给六个结构各加 `#[serde(default)]`，并加两条测试：
`a_partial_nested_object_keeps_the_other_settings`（只写一个字段，其余必须留住）
与 `the_defaults_round_trip`（本构建写出的文件必须读得回来）。前者在修之前**是红的**。

验收：**2 条新测试**（556 = 554 + 2）。全工作区 **556** 项测试全绿、
clippy 零自有警告、生成器幂等（`menu.rs` `d72a9524` / `keymap.rs` `75c5c9e0` 未变）、
冒烟开一份 Rust 文件存活 7 秒且 stdout/stderr 各 0 字节。

### 8.14 默认编码是一个「文件编码」，不是一个 codec ✅

§8.12 的缺口表里躺着两条相邻的账：「默认编码设置没有 BOM 选项」与「缺
`Unicode (UTF-16)` / `Unicode (UTF-32)`」。第一条查下去发现问题比记账时更大。

#### 上游的事实

`FormatSettingsView` 的 *Default text encoding* 选择器绑的不是一个 codec，是一个
`FileEncoding`（codec + `withUTF8BOM`），它由两个 `@AppStorage` 合成：

```swift
get: { FileEncoding(encoding: …, withUTF8BOM: self.saveUTF8BOM) }
set: { self.encoding = …; self.saveUTF8BOM = $0.withUTF8BOM }
```

而 `EncodingManager.defaultEncoding` 的 getter 里有一道闸：
`withBOM = (availableEncoding == .utf8) && self.defaults[.saveUTF8BOM]` ——
只有 UTF-8 能有这个 mark。`saveUTF8BOM` **没有任何自己的控件**：全仓只有
`EncodingManager` 与这个选择器读写它。

#### SubText 查出来的三个问题

1. **默认编码只存 codec**，所以选择器里没有 *Unicode (UTF-8) with BOM* 这一条。
2. **`edit.save_utf8_bom` 是个没人读的开关**。设置窗口 Edit 面板里有一个
   *Add a byte-order mark when saving as UTF-8*，而 `Document::untitled` 无论
   默认编码是什么都写 `bom: Bom::None` —— 打开它，新建文档照样没有 mark。
   这正是工作原则第 3 条要防的东西：**一个看起来生效的开关**。
3. 顺带查出**上游 Edit 面板根本没有「Saving」这一段**（它只有 Indentation /
   Whitespace / Comment / Content parse 四节）。那个开关不只是死的，还是**自造的**。

#### 改动

| 位置 | 之前 | 现在 |
| --- | --- | --- |
| `Document::untitled` | 收 `TextEncoding`，恒 `Bom::None` | 收 `FileEncoding`，`bom` 取自它（`FileEncoding::bom()` 顺带给 UTF-16/32 它们的 mark，即上游的 `hasBOM`） |
| `Settings` | 只有 `encoding()` | `default_file_encoding()` / `set_default_file_encoding()`：两半**一个门**写、一个门读，读取时按 codec 收口 |
| Format 选择器 | 列 codec | 列 `FileEncoding::menu_items(&list)`：UTF-8 后面紧跟 *with BOM*，分隔符画成横线 |
| `pick_list` | 只吃 `Vec<SharedString>` | 新增 `pick_list_with_gaps`（`None` 画一条 `divider`），原函数转发 —— §8.12 记的「选择器里没有分隔线」一并关掉 |
| Edit 面板 | 多一个 *Saving* 段 | 删掉，回到上游的四节 |

`FileEncoding::menu_items` 是**唯一**做「UTF-8 之后插 with-BOM」的地方，
`menu_groups` 改成「把 `menu_items` 按分隔符切开」—— 于是菜单与选择器是同一串条目的
两种切法，构造上不可能不一致。`the_separators_survive_the_cut_and_join` 现在同时断言
`menu_items` 与 `menu_groups` 都等于那个独立的、逐项映射的 oracle。

#### 验证：四种设置下的新文档

临时探针在 `new_document` 里打印新文档的 codec 与 mark（已移除，
`git diff | grep -c PROBE` = 0）：

| 设置文件 | 实测 |
| --- | --- |
| `default_encoding: "utf-8"` | `encoding=utf-8 bom=None` ✅ |
| `utf-8` + `edit.save_utf8_bom: true` | `encoding=utf-8 bom=Utf8` ✅ **之前是 None** |
| `default_encoding: "utf-16le"` | `encoding=utf-16le bom=Utf16Le` ✅ **之前是 None** |
| `shift_jis` + `save_utf8_bom: true` | `encoding=shift_jis bom=None` ✅ 闸门生效 |

#### 仍然记账的一条

**缺 `Unicode (UTF-16)` / `Unicode (UTF-32)` 两条菜单项**没在这一轮做，而且理由更清楚了：
上游这两条的意思是「让 BOM 决定字节序」，而 SubText 的 `FileEncoding` 把字节序做成显式的
（`utf-16le` / `utf-16be`）—— 要补这两条，先得给 `FileEncoding` 一个「未指定字节序」态
并定义写出规则，是一个独立改动。这一轮做的是**另一半**：让默认编码记住它的 mark。

验收：**5 条新测试**（559 = 556 + 3 core + 2 app）——
core 的 `a_new_document_carries_the_mark_of_its_default_encoding`（三种默认编码）、
`the_default_mark_is_written_to_disk`（字节真写到磁盘、再读回来还是 Utf8），
app 的 `the_default_file_encoding_pairs_the_codec_with_its_mark`（两半同写、换 codec 清 mark、
手改的 flag 被闸门挡掉）；`the_separators_survive_the_cut_and_join` 加了 `menu_items` 的断言。
全工作区 **559** 项测试全绿、clippy 零自有警告、生成器幂等
（`menu.rs` `d72a9524` / `keymap.rs` `75c5c9e0` 未变）、
冒烟开一份 Rust 文件存活 7 秒且 stdout/stderr 各 0 字节。

### 8.15 文档窗口的状态栏与导航栏 ✅

用户要求「优先对齐桌面布局、桌面UI，必须做到完全一致」，于是把上游的窗口层次整个读了一遍。
结果发现 **SubText 的这两条栏都不是上游的东西**。

#### 上游的层次（`WindowContentViewController` + `ContentViewController` + `EditorViewController`）

```
DocumentWindow
└─ WindowContentViewController   （NSSplitViewController，autosaveName "WindowContentSplitView"）
   ├─ sidebarViewItem            （sidebarWithViewController，可折叠）
   ├─ contentViewItem            （ContentViewController）
   │  ├─ DocumentViewController  （NSSplitViewController：分屏编辑器，每屏 minimumThickness 100）
   │  │  └─ EditorViewController ×N
   │  │     ├─ navigationBarItem （NavigationBar，高 24）
   │  │     └─ 文本视图
   │  └─ [bottom accessory] StatusBar   ← 挂在**内容项**上
   └─ inspectorViewItem          （inspectorWithViewController，minimumThickness 200）
```

两条结论：

1. **状态栏属于内容列**，所以它到侧栏边缘为止，侧栏与检查器是整窗高度的。SubText 此前把它
   放在窗口最底部，于是它横跨侧栏与检查器 —— 上游不是这样。
2. **导航栏属于每个编辑分屏**，内容是「关闭分屏 · 分隔线 · 大纲导航器 · 撑开 · 分屏按钮」。
   SubText 此前在窗口底部放了一条显示「语法 — 换行符」的文字，与上游的导航栏毫无关系。

#### 状态栏：上游的项

```
[不可编辑徽标] [Lines: n  Characters: n  Words: n  Location: n  Line: n  Column: n] … [文件大小] │ [编码] │ [换行符]
```

- 六个计数各由一个开关控制（Window 面板的 *Status Bar* 一节）。**此前那六个开关没有任何代码读**，
  是六个改了没反应的开关 —— 与 §8.14 的 BOM 开关同一类缺陷。
- 三个计数在**有选区时**显示 `entire (selected)`（上游 `EditorCounter.Count.formatted`）。
- 三个位置量的**进制各不相同**，这是上游的形状：`Location` 0 基、`Line` 1 基、`Column` **0 基**，
  单位都是 Swift 的 `Character`（扩展字形簇），所以 `\r\n` 算一列、`🐱` 也算一列。
  `textops::line_and_column` 数的是 `\n` 字节与标量，答的是另一个问题（Go to Line 的），
  所以新写了 `textops::status_position`，并把上游 `StringCountingTests.swift` 的三组向量
  逐条搬成测试。**顺带修掉一个真 bug**：原来那个只认 `\n`，用 CR / NEL / LS / PS 写的文档
  行号永远是 1。
- 文件大小与编码/换行符选择器都在这一行；选择器就是 `FileEncoding::menu_items`（与菜单同一份列表）。

#### 导航栏：上游的项

```
[关闭分屏 (xmark)] [分隔线] [上一个] [下一个] [大纲菜单] … [分屏按钮（右键改方向）]
```

- 关闭按钮在只有一个分屏时**不可见**（opacity 0 + disabled + 不接收点击）；分隔线在大纲为空时也不显示。
- 上一个/下一个的箭头随分屏方向换：竖直分屏用左右箭头。判据与命令走同一次 `previous_item` / `next_item`。
- 大纲菜单列出所有条目，选中当前条目。
- 分屏按钮的**右键菜单**切换方向，点击仍然是分屏 —— 与上游一致。
- 上游把方向编码成「一个符号旋转 -90°」；上一轮误以为 GPUI 没有旋转，用了两个符号
  （`SquareSplitVertical` / `SquareSplitHorizontal`）。**§8.16 发现 `Icon::rotate` 一直存在**，
  已改回「一个符号转 -90°」。

#### 编码切换要问「转换还是重解释」

上游 `changeEncoding(_:)` 是**菜单与状态栏选择器共用的一个动作**，而它走
`askChangingEncoding`：文档没有文件或文本为空时直接改，否则弹一个
「Do you want to convert or reinterpret this document using "X"?」带
*Convert* / *Reinterpret* / *Cancel* 三个按钮。SubText 此前**菜单项直接转换、不问**。
现在两处都走 `Workspace::ask_changing_encoding`。

#### 记账的偏差

| 偏差 | 说明 |
| --- | --- |
| ~~分屏方向用两个符号而非旋转一个~~ | **§8.16 关掉了。** 当时写的是「GPUI 的 `Styled` 没有 `rotate`，`Icon` 也没有」—— 后半句是错的，`Icon::rotate(impl Into<Radians>)` 一直存在（`gpui-base` 的 Accordion 自己就用它转 chevron）。现在是一个符号转 -90°。 |
| 转换有损时只有一个对话框 | 上游是两步：先问转换/重解释，发现不能无损再弹第二个 sheet（多一个 *Change Encoding* 按钮）。SubText 把警告并进第一个对话框 —— 用户在选之前就看见，且只有一个界面。判断有损用的是 `TextEncoding::encode` 自己的错误标志，也就是写出时真正会碰到的那个。 |
| 换行符菜单项带长名 | 上游的弹窗项只写缩写、长名在 tooltip 里；GPUI 菜单项没有 tooltip，所以写成 `"Windows (CRLF)"`，与 Format ▸ Line Endings 子菜单同形。 |
| 状态消息占用计数那一格 | 上游没有状态消息行（它用 alert）。SubText 自己的「未实现」提示在 4 秒有效期内**替换**计数，而不是新增一条车道 —— 于是没消息时布局与上游完全相同。 |
| ~~文档检查器的 `File Size` 还是文本的编码长度~~ | **§8.16 关掉了。** 检查器按上游重排后，`Size` 读的是 `FileAttributes::read` 的 `metadata.len()`，也就是磁盘上的那个量。 |
| 检查器的行集与上游不同 | 上游是「File（Created / Modified / Size / Tags / Permissions / Owner / Full Path）· Document（Encoding / Line Endings / Mode）· Counts · Character」；SubText 现在给的是另一套。同为检查器面板的账。 |

验收：**3 条新测试**（562 = 559 + 3）——core 的 `the_status_position_has_three_different_bases`、
`the_status_column_counts_graphemes`、`every_line_ending_moves_the_line_number`。
全工作区 **562** 项测试全绿、clippy 零自有警告、生成器幂等、
冒烟（临时探针已移除，`git diff | grep -c PROBE` = 0）：
侧栏 + 检查器 + 导航栏 + 状态栏全开、开一份含大纲的 Rust 文件存活 8 秒、stdout/stderr 各 0 字节；
派发两次 `ChangeEncoding`（第二次时对话框已开）同样存活。

### 8.16 文档检查器按上游重排成四段 ✅

§8.15 把窗口框架读完时，顺手发现检查器的行集是 SubText 自己编的（十二行：
File Name / File Size / Encoding / BOM / Line Endings / Syntax / Lines / Characters /
Words / Paragraphs / UTF-16 Units / UTF-8 Bytes）。上游不是这样，于是这一轮按
`DocumentInspectorView` 重排，并把支撑它的三块数据补真。

#### 上游的四段

```
File            Created · Modified · Size · Tags · Permissions · Owner · Full Path
Text Settings   Encoding · Line Endings · Mode
Count           Lines · Characters · Words · Location · Line · Column
Character       Code Point(s) · Name · Block · Category
```

- 四段都是 `DisclosureGroup`，标签 12pt semibold、次级色，三角在**前**；默认全展开。
  上游用 `InspectorDisclosureGroupStyle` 与 `InspectorLabeledContentStyle` 定制，
  行是 `LabeledContent`：标签在左列、值贴右缘。
- `Count` 的三对计数与状态栏是同一份（`entire (selected)`），三个位置量没有选中半。
  上游的 `EditorCounter.Result.formattedValue` 就是状态栏用的那个方法 —— 两处一个来源。
- `Character` 只在选区**正好是一个字符**（一个扩展字形簇）时给数据：先列 code point
  （单数个时标签用单数 `Code Point`，多个时用复数 `Code Points`），
  **只有单标量**才再给 Name / Block / Category；没有选区时整段写 *Not selected*。
- `Mode` 是当前语法。上游的 `Mode` 是「kind 或语法名」，kind 取它的本地化标签；
  SubText 的语法就是同一批 bundle，名字同名。

#### 顺带查出的三个真 bug（计数）

这三个数是状态栏与检查器**共用**的，所以一直是错的，只是没人量过：

| 量 | SubText 原来 | 上游 | 说明 |
| --- | --- | --- | --- |
| `Characters` | `text.chars().count()`（标量） | 扩展字形簇 | `😀🇯🇵a` 是 3 不是 4；`👍🏻` 是 1 不是 2；`\r\n` 是 1 不是 2 |
| `Lines` | `split_lines(text).len()` | 结尾的换行符**结束**它那一行，不开新的一行 | `"a\nb c\n\n"` 是 3 不是 4；`""` 是 0 |
| `Words` | 按空白与 CJK 标点切 | UAX #29 词边界（上游走 ICU） | `plain-text` 是 2 不是 1 |
| `Unicode Scalars` | **等于 `characters`**（复制） | 标量数 | 这个字段以前就是另一个字段的副本 |

上游 `StringCountingTests.swift` 的向量逐条搬成了测试。**`Words` 有一处真实差异**，
已在缺口表记账并写进测试注释：ICU 的分词带词典，UAX #29 只有规则，
`こんにちは` 在规则下是 5、ICU 下是 1。

#### 补真的三块数据（都是照搬上游的表）

1. **Unicode 区块表**：原来只有 72 条「文本编辑器大致会遇到的」区块，其余一律报
   `No Block` —— 一个自造的值。现在是从上游 `Unicode.UTF32.CodeUnit+BlockName.swift`
   逐条搬来的 **346 条**（Unicode 17.0），未分配返回 `None`，`No Block` 由面板自己画。
2. **General_Category**：原来是靠 `char` 的谓词拼出来的近似（`Ⅻ` 会被算成 `Nd`，
   `½` 也是）。现在用 `unicode-properties` 的真表，30 个类别与上游 `longName` /
   `shortName` 两张表逐条对齐（`Uppercase Letter (Lu)`）。
3. **控制字符名**：Unicode 对 C0/C1 只写 `<control>`，上游自带 ASCII/ISO 6429 的名字表
   （`NULL`、`NEXT LINE`…），照搬 66 条。
4. **Finder 标签**：**没做**，见缺口表。`Tags` 这一行因此没有画 —— 一行永远填不上的
   比没有这一行更糟。

#### 新增：文件属性

`subtext-core::fileattrs`，按上游 `DocumentFile` 的 `FileAttributes` /
`FilePermissions` 移植：Created / Modified / Size / Permissions（`644 (-rw-r--r--)`，
含 setuid/setgid/sticky 的 `s`/`S`/`t`/`T`）/ Owner。日期用 `jiff` 按本机时区格式化，
形状照上游 `.formatted(date: .abbreviated, time: .shortened)` 的英文结果写死
（`Oct 15, 2026 at 11:04 PM`）—— SubText 没有本地化机制。Owner 走 `getpwuid_r`
（unix；Windows 没有对应调用，返回 `None`）。Full Path 行带 *Show in Finder* 按钮。

#### 顺带关掉的一条旧账

§8.15 记的「分屏方向用两个符号而非旋转一个」**关掉了**：`Icon::rotate` 一直存在
（`gpui-base` 的 Accordion 自己就用它转 chevron），只是上一轮 grep 没找到。
现在是一个符号转 -90°，与上游的 `.rotationEffect(.degrees(isVertical ? -90 : 0))` 同形。
同一个发现也让四段的三角用「右→下」两个位置表达，而不是两个符号。

#### 记账的偏差

| 偏差 | 说明 |
| --- | --- |
| 没有 `Tags` 行 | 上游从 `com.apple.metadata:_kMDItemUserTags` 扩展属性读，值是 **binary plist**（`PropertyListDecoder` 解成 `[String]`，每项 `"名字\n颜色序号"`）。Rust 侧要一个 bplist 解析器或新依赖，且非 macOS 没有对应属性。**这一轮没做**，行也没画。 |
| `Words` 与 ICU 不同 | 见上。规则内的用例全对，差别只在「词之间不写空格」的文字。 |
| `Full Path` 是尾部截断 | 上游 `truncationMode(.middle)` 截中间，还带 `help` tooltip 且文本可选。GPUI 只有尾部截断，`div` 没有 tooltip，`div` 也不可选。 |
| code point 没有描边框 | 上游把每个 code point 放进 `RoundedRectangle.strokeBorder` 的方框里；SubText 用空格连起来。 |
| 四段没有展开动画 | 上游的 `DisclosureGroup` 带 SwiftUI 的默认动画。`Accordion` 的 item 用 `spring` + `MotionReveal` 动画，但它的 chevron 在**后**、标签样式也不对，所以这一轮用的是同一套基元里无样式的那一个（`AccordionTrigger`），动画一并舍去。 |
| 检查器的 `Created` 在部分平台为空 | `std::fs::Metadata::created()` 在 Linux 上没有 birthtime 时报错，那一格就是空的。 |
| 检查器的宽度是固定的 | 上游 `inspectorViewItem.minimumThickness = 200`，SubText 是 12–26 rem 可拖。§8.13 已记账。 |

验收：**17 条新测试**（579 = 562 + 17）——core 的 `fileattrs` 6 条（三种权限渲染、
setuid/setgid/sticky、真文件读回来、不存在的文件、时间戳形状），`charinfo` 11 条
（区块表 346 条且升序不重叠、显示名、控制字符名、无名的标量、谓词会答错的类别、
`CharacterReport` 的单/多标量、`Count::formatted`、三对计数、以及上游
`StringCountingTests.swift` 的四组向量）。
全工作区 **579** 项测试全绿、clippy 零自有警告、生成器幂等、
冒烟（临时探针已移除，`git diff | grep -c PROBE` = 0）：
侧栏 + 检查器 + 导航栏 + 状态栏全开、开一份含中日文与 Rust 代码的文件存活 9 秒、
stdout/stderr 各 0 字节。**并且第一次真正看到了界面**：用 `CGWindowListCopyWindowInfo`
取窗口号 + `screencapture -l` 截了应用窗口，逐段核对了四段的每一行、
折叠态（`File` 收起后行消失、三角转回朝右）与单标量 / 多标量两种 Character 段。

### 8.17 Inspect Character 改成锚定选区的 popover ✅

§8.16 补 `ScalarInfo` 时顺手发现：`Text ▸ Inspect Character`（⌃⌘I）在 SubText 里是一个
**模态对话框**，十行平铺的键值；上游不是窗口，也不长这样。

#### 上游是一个 popover

```swift
// EditorTextViewController.showSelectionInfo
guard self.textView.selectsSingleCharacter,
      let character = self.textView.selectedString.first
else { return assertionFailure() }

let view = NSHostingView(rootView: CharacterInspectorView(character).padding(14))
...
present(popoverController, asPopoverRelativeTo: positioningRect,
        of: textView, preferredEdge: .minY, behavior: .semitransient)
```

四个要点，SubText 现在都有了：

1. **锚在选中字符上**，`preferredEdge: .minY`（在上方，装不下时翻到下方）。
   用 `Positioner::side(range_to_bounds(selection)).placement(Top)`，
   `Align::Center` 是它的默认值 —— 与 `preferredEdge: .minY` 同形。
2. **`.semitransient`**：点外面就关，且那一次点击不落到下面。
   popover 底下铺一层整窗的接收层，它的 mouse-down 关掉 popover。
3. **Escape 关**。popover 故意不抢焦点（上游也不抢，抢了就把正在编辑的会话打断），
   所以 Escape 只能在**捕获阶段**从工作区根上拿 —— 与命令面板同一手法。
4. **选区不是一个字符就什么都不做**（上游是 `assertionFailure()`）。
   SubText 在状态栏说明原因而不是吞掉按键，与 `Straighten Quotes` 同一条理由：
   菜单项只有固定的 `disabled`，跟着选区动就得每次重建 258 项菜单栏。

#### 面板的内容（`CharacterInspectorView`）

```
[大字形 64pt]  <名字>，或 <a letter consisting of N characters>，或 Unknown
               Code Point:  U+0041 ／ U+1F600 (D83D DE00) ／ + deprecated 徽标
               Block:       Basic Latin          ← 按钮，点一下换成 Blocks.txt 的名字
               Category:    Uppercase Letter (Lu)
               Version:     Unicode 1.1          ← 只在 Unicode 给这个标量定了版本时
```

- **多标量**时改成每个标量一个可折叠行（默认收起），展开后给字形 + Block + Category；
  单标量才给 Code Point 与 Version —— 上游的 `items: [.block, .category]`。
- **变体**（两个标量、第二个是变体选择符或肤色修饰符）不算「复杂」：
  名字保留，后面括号里写 `Emoji Style` / `Text Style` / `Skin Tone I-II`…。
- **控制字符**的大字形画的是 Control Picture（`␀`、`␊`、`␡`）并改用次级色，
  因为字符本身看不见。
- **deprecated 徽标**：`Unicode.Scalar.Properties.isDeprecated` 为真时，Code Point 旁边
  一个描边小字。

#### 新表：`unicode_tables.rs`（生成文件）

面板要三个属性，Rust 标准库与树里任何 crate 都没有：`age`（哪个 Unicode 版本收的）、
`isDeprecated`、`isVariationSelector`。**Swift 的标准库三个都有，而它正是上游读的那份
数据**，所以新写 `tools/generate_unicode_tables.swift` 从它生成
`crates/subtext-core/src/unicode_tables.rs`（age 1787 段、deprecated 8 段、
variation selector 4 段，各是「极大同值区间」+ 二分查找）。生成的文件入库，
所以**构建 SubText 不需要 Swift**；生成器自己带测试守卫（区间升序不重叠 + 几个实测值）。

这与 `menu.rs` / `keymap.rs` 是同一条路：**必须一致的东西做成数据，原样复用**。

#### 顺带修掉一个真 bug（探针发现的）

`render_scalar_row` 里按 `index` 取 `inspect_scalar_open[index]`，而 `inspect_selection`
只初始化了 `inspect_block_localized` —— 多标量的字符一选中就
`index out of bounds: the len is 0 but the index is 0`，整个进程 abort。
`inspect_selection` 现在把两个向量一起按标量数建好，取值处也改成
`.get(index).copied().unwrap_or(false)`。**这是这一轮唯一的 panic**，
是截图探针跑多标量用例时抓到的 —— 只看「进程存活」看不出来，因为它崩在交互之后。

#### 记账的偏差

| 偏差 | 说明 |
| --- | --- |
| 大字形不是衬线体 | 上游 `.fontDescriptor.withDesign(.serif)`，64pt。主题只有 `font_family` 与 `mono_font_family`，没有衬线一族，所以用界面字体。 |
| 字形没有文本选择的装饰 | 上游把每个 code point 放进描边框、`textSelection(.enabled)`。GPUI 的 `div` 不可选。 |
| 面板宽度不是内容自适应 | 上游 `intrinsicContentSize` 定宽；GPUI 里由内容与 `Positioner` 的视口夹取共同决定。 |
| 没有 `⌘I` 之外的入口 | 上游的 popover 还会 `showFindIndicator` 闪一下选中范围；GPUI 没有那个 API。 |

验收：**9 条新测试**（588 = 579 + 9）——core 的 `unicode_tables` 2 条（区间升序不重叠、
几个实测值）、`charinfo` 7 条（控制字符的 Control Picture、变体不是复杂字符、
复杂字符报标量数、无名角色没有标题、Code Point 行的代理对、Version 行、
deprecated 标志）。
全工作区 **588** 项测试全绿、clippy 零自有警告、生成器幂等（`menu.rs` `d72a9524` /
`keymap.rs` `75c5c9e0` 未变；`unicode_tables.rs` 连跑两次 sha256 不变）、
探针已移除（`git diff | grep -c PROBE` = 0）、冒烟存活 8 秒且 stdout/stderr 各 0 字节。
**五种情形逐一截了应用窗口核对**：ASCII（`A`）、变体（`✔️` → `HEAVY CHECK MARK
(Emoji Style)` + 两行标量）、复杂（`🏴☠️` → `<a letter consisting of 4 characters>`
+ 四行标量）、控制字符（`\0` → 灰的 `␀` + `NULL`）、未分配（`U+34000` → `Unknown`
+ 代理对 `D890 DC00` + `No Block` 禁用按钮 + 没有 Version 行）。
行数 37,284 → 39,741（`unicode_tables.rs` 1903、`charinfo.rs` 1439、`workspace.rs` 5429）。

### 8.18 自定义包围 —— 以及五个内置包围一直包错了对象 ✅

`Text ▸ Surround Selection With ▸ Custom…`（⇧⌘L）是**文本菜单最后一个占位提示**，
触发时在状态栏说「Custom surrounds are not implemented yet」。这一轮把它做出来，
顺手发现**同菜单里那五个内置命令一直在包行，而上游包的是选区**。

#### 上游的 sheet

```swift
@IBAction func surroundSelection(_ sender: Any?) {
    self.window?.beginSheet {
        CustomSurroundView(pair: self.customSurroundPair) { [weak self] pair in
            self?.surroundSelections(begin: pair.begin, end: pair.end)
            self?.customSurroundPair = pair
        }
        .scenePadding()
    }
}
```

`CustomSurroundView` 是三行东西：`Surround with:`（semibold）、
`Begin:` 与 `End:` 两个 48pt 输入框（`End` 的 placeholder 是 `Begin` 的值），
再加一对等宽按钮（Cancel / OK）。提交规则两条：

- **begin 为空不是一对**（上游 `guard !begin.isEmpty else { return NSSound.beep() }`）；
- **end 为空表示「和 begin 一样」**，于是 `"` 打一次就得到 `"…"`。

SubText 的两处有意偏差：begin 为空时**把 OK 置灰**而不是让按钮按下去再响一声
（同一条规则，多一个看得见的理由），`End` 的 placeholder **跟着 `Begin` 走**
（上游是构造时传 `prompt:`，打开后就冻住了）。配对存在
`Settings::custom_surround_begin/end`，对应上游的两个 `UserDefaults` 键。

#### 顺带查出的真 bug：包围的是行，不是选区

SubText 原来调用的是自己写的 `textops::surround_lines` —— 「把选中的**每一行**
用前后缀包起来，空行跳过」。上游的 `surroundSelections(begin:end:)` 走的是
`String.surround(in: selectedRanges, begin:end:)`，包的是**选区本身**。

上游自己的 fixture 把这件事钉死了（`StringEditingTests.surround`）：

```
"foo\nbar\nbaz"  +  ranges [0..<3, 4..<4, 8..<11]  +  "<" ">"
  →  strings         ["<foo>", "<>", "<baz>"]
  →  applying(…)     "<foo>\n<>bar\n<baz>"
```

两个后果，都是用户能看见的：

| 情形 | SubText 原来 | 上游 |
| --- | --- | --- |
| `hello world` 里只选中 `world`，按 ⇧⌘9 | `(hello world)` —— 整行被包 | `hello (world)` |
| 没有选区、只有一个光标，按 ⌘' | 整行被包成 `'…'` | 在光标处插入 `''`，光标落在两半中间 |

第二条尤其重要：`rangesForUserTextChange` 返回的是 `insertionRanges`，
光标就是一个**空 range**，而空 range 和别的 range 一样处理 —— 所以那五个内置命令
本来也是「打一对引号」的快捷键。新函数 `textops::surround` 就是照这个语义写的，
`surround_lines` 与它的测试一并删掉（没有别的调用者，也没有别的上游对应物）。

#### 记账的偏差

| 偏差 | 说明 |
| --- | --- |
| 对话框有标题 | 上游的 sheet 没有标题（一个光秃秃的 `beginSheet { … }`）。GPUI 的 `Dialog` 要一个，用了 *Custom Surround* —— 与 §8.11 的 *Encoding List* 同一条。 |
| begin 为空时按钮置灰而不是响一声 | 同一条规则，多一个看得见的理由。Enter 走的是对话框自己的动作而不是那个按钮，所以 `submit()` 里**再查一次**。 |
| 记住的是一对字符串，不是每个文本视图一份 | 上游除了 `UserDefaults` 还在 `EditorTextView` 上留一份 `customSurroundPair`（每个文档窗口一份）。两者在提交时同时写、内容总是一样，所以 SubText 只留了持久化的那一份。 |

验收：**5 条新测试**（593 = 588 + 5）——core 的 `surround_wraps_the_selection_not_the_line`
（上游 fixture 的三种 range）、`surround_keeps_the_delimiters_off_a_multibyte_boundary`、
`a_custom_surround_pair_can_be_anything`；app 的 `an_empty_beginning_is_not_a_pair`、
`an_empty_end_means_the_same_as_the_beginning`。
全工作区 **593** 项测试全绿、clippy 零自有警告、两个生成器都幂等、
探针已移除（`git diff | grep -c PROBE` = 0）、冒烟存活且 stdout/stderr 各 0 字节。
**三种情形截了应用窗口核对**：sheet 打开（`Begin:`/`End:` 两个框、OK 因为 begin 为空而置灰）、
提交后正文变成 `hello **world**`（**只包选区**，行模式的旧实现会得到 `**hello world**`）、
重开 sheet（两个框带上次的 `**`，OK 可用），并核对了 `Settings.json` 里
`custom_surround_begin/end` 都写成了 `**`。
行数 39,741 → 40,087。

### 8.19 高级计数按上游重做成「选项 + 胶囊」 ✅

`Text ▸ Advanced Character Count…` 在 SubText 里原来是一个**静态对话框**（标题
*Document Statistics*，九行固定的数字），而它上游是一个**带选项的活计数器**。
这一轮把它整个换掉。

#### 上游的三个部件

1. **一个 sheet**（`CharacterCountOptionsSheetView`）：一份选项面板 + 一对按钮，
   确认按钮写的是 **Start**。
2. **一颗胶囊**（`AdvancedCharacterCounterView`）：浮在正文右上角，
   左边是计数、右边一个齿轮；齿轮打开 **popover**，里面是**同一份选项面板** ——
   一边看数字一边调选项。右键菜单两项：*Copy*（复制那个数字）与 *Stop Count*。
3. **一份选项面板**（`CharacterCountOptionsView`），三个 whitespace 复选框 +
   *Unit* 菜单 +（选到 Byte 时）*Encoding* 菜单 +（不是字形簇时）*Normalization* 开关与菜单。

计数本身是**派生的**：有选区数选区，没有就数整篇（上游 `counter.count`），
所以打字时它自己就跟着走，不需要谁去通知它。

#### 算法：`CharacterCountOptions::count`

上游的 `String.count(options:)`，顺序就是面板上复选框的顺序：

```
空串 → 0（先于任何变换，所以空文档是 0 字节而不是「失败」）
忽略换行 → 去掉 \R（\r\n | \n | \v | \f | \r | NEL | LS | PS）
忽略空白 → 去掉 [\t\p{Zs}]（**不含换行**，这正是两个开关能分开的原因）
连续空白算一个 → 把 [\t\n\f\r\p{Z}]{2,} 折成一个空格（两个忽略都开时这个开关是禁用的）
规范化 → 按所选形式
再按单位数：字形簇 / 标量 / UTF-16 / 字节（字节是唯一会失败的：编不出来就是 nil）
```

两个容易抄错的点：**ICU 的 `\s` 是 `[\t\n\f\r\p{Z}]`，不是 Rust 的 `\p{White_Space}`**
（后者多算垂直制表与 NEL），以及 `\R` **比编辑器认得的六种换行符更宽**（多出 `\v` 与 `\f`）。
两处都写成显式的字符类，并各有一条测试盯着。

#### 记账的偏差

| 偏差 | 说明 |
| --- | --- |
| 规范化菜单只有 4 项，上游 7 项 | NFKC Case-Fold 与两个 HFS+ 形态是 SubText 没有的 Unicode 数据，`normalize` 对它们是**拒绝而不是近似**。列出来会得到一个静默错误的计数，所以不列。Text ▸ Unicode Normalizations **菜单**仍然列全 7 项并说明那三项为什么不可用。 |
| 胶囊不能拖动 | 上游的 `DraggableHostingView` 可以把它拖到任何地方；SubText 固定在正文右上角（也就是新开一颗时的位置）。 |
| 没有 macOS 26 的玻璃效果 | 上游 `.glassEffect(in: .capsule)`。用主题的 `popover` 底色 + 描边 + 阴影，读起来是同一颗胶囊。 |

验收：**9 条新测试**（602 = 593 + 9）——core 的
`the_whitespace_options_narrow_the_count`（上游 `AdvancedCountingTests` 的六个期望）、
`the_unit_changes_what_is_counted`、`normalization_is_applied_before_the_unit_is_read`、
`the_byte_unit_can_fail`、`the_line_endings_the_count_drops_are_icurs_r_not_the_editors_six`；
app 的 `the_byte_unit_is_spelled_out_and_the_rest_are_characters`、
`the_count_is_grouped_like_the_platform_groups_it`、
`the_normalization_menu_offers_only_the_forms_this_build_can_apply`、
`every_unit_and_every_encoding_is_offered`。
全工作区 **602** 项测试全绿、clippy 零自有警告、两个生成器都幂等、
探针已移除（`git diff | grep -c PROBE` = 0）、冒烟存活且 stdout/stderr 各 0 字节。
**五种情形截了应用窗口核对**：Start sheet（三个复选框 + Unit 菜单 + 说明 + Cancel/Start）、
胶囊数选区（`5 character(s)`）、切成 Byte（`5 bytes`）、选到 ASCII 编不出的日文
（红三角 + `failed`）、齿轮 popover（同一份选项面板）。
行数 40,087 → 41,117（`count_panel.rs` 508）。

### 8.20 补全 —— 以及三个被 base64 吃掉的快捷键 ✅

`Edit ▸ Complete`（⌥⎋）此前在状态栏说「That selection command is not implemented yet」，
Mode 面板里那三个补全开关（*Standard words* / *Words in document* / *Words defined in
syntax*）**没有任何代码读** —— 与 BOM 开关、状态栏六开关同一类缺陷。这一轮把补全做出来，
三个开关里的两个真的生效了。

#### 上游的候选算法

`EditorTextView.completions(forPartialWordRange:indexOfSelectedItem:)`，两个来源：

```
partialWord = 光标处的词
文档里的词   ← 正则 \b<转义的 partial>\w+\b 扫全文（**大小写敏感**）
语法定义的词 ← 语法 Completion.json 里以 partial 开头（**大小写不敏感**）
去重 → 只剩一个候选且它就是输入本身时不显示
```

**要补全的那一段**（`rangeForUserCompletion`）是两段叠出来的，两段都**实测**过
（`clang`/`swiftc` 起一个真的 `NSTextView` 问它）：

1. **AppKit 自己的**：光标前那串字母、数字、`_`、**`.`**。实测 `a.b` 给整串、
   `@inter` 给 `inter`、组合附标跟着它附着的字母走。
2. **上游再往左扩**，条件是「补全还接得下去」：字母数字、`_`、以及**语法补全词的
   首字符**。它的集合里**没有 `.`**，所以这一段其实是**缩短** AppKit 的答案 ——
   `a.b` 只补 `b`。实测也确认了：`@inter` 即使语法里有 `@interface` 也只补 `inter`，
   因为「没有该停的字符」时它原样返回 AppKit 的答案。

插入后：**词里有括号就选中括号里面**（`foo()` → 光标停在括号中间），
上游 `insertCompletion` 的贪心正则语义。

#### 顺带查出：三个快捷键被 base64 吃掉了

Storyboard 里凡是**控制字符**做键位的地方，Interface Builder 不能写成 XML 属性，
就存成 `<string key="keyEquivalent" base64-UTF8="YES">`。当初提取 `menu.json` 时没解这层，
于是三条键位读成了空串，`keymap.rs` 里一直少三条：

| 菜单项 | 上游 | 之前 | 现在 |
| --- | --- | --- | --- |
| Edit ▸ Delete | ⌘⌫（`CA` = 0x08） | 无 | ✅ |
| Edit ▸ Delete Line | ⇧⌘⌫（`CA`） | 无 | ✅ |
| Edit ▸ Complete | ⌥⎋（`Gw` = 0x1B） | 无 | ✅ |

**`Delete Line` 那条最刺眼**：命令本身早就能用，只是快捷键丢了。
生成器的 `NAMED_KEYS` 现在多两个控制字符（`backspace` / `escape`），
快捷键表从 72 条变成 **75 条**。

`Edit ▸ Delete` 也顺手做实了：它此前在状态栏说「Handled by the text engine」，
而 AppKit 的文本视图**并不**绑定 ⌘⌫ —— 绑上它反而会把按键变成一句提示。
上游的 `delete:` 就是「删选区，否则删光标前一个字符」，所以 SubText 现在真的删
（`textops::delete_backward`，单位是**扩展字形簇**：⌘⌫ 在 `👍🏻` 后面一次删掉整个）。

#### 记账的偏差

| 偏差 | 说明 |
| --- | --- |
| 候选列表没有虚拟化 | 上游是 AppKit 的弹出列表；SubText 用一列普通行放进 `max_h` + 滚动。行的数量由共享前缀的词数决定，通常很小。 |
| 没有 IME 合成态的判断 | 上游 `performCompletion` 会先问 `hasMarkedText()`，正在拼字时不弹。GPUI 的编辑器不暴露这个状态。 |
| `Standard words` 依旧是关的 | AppKit 的系统词表在系统文本引擎里，Rust 侧够不着 —— Mode 面板早就把它禁用并写了理由，这一轮没有改变这一点。 |
| 自动补全（*Suggest completions while typing*）未做 | 上游在 `insertText` 后 250ms 防抖、且要求光标前是词字符、词长 ≥ 3、光标后不是字母数字。SubText 只做了 ⌥⎋ 这一半；那个开关仍是禁用的（面板里写明了）。 |

验收：**12 条新测试**（614 = 602 + 12）——core 的 `completion` 10 条
（词范围的两段与四个实测向量、语法首字符决定停在哪里、文档词与语法词、一字符符号
不搜全文、去重与顺序、单候选等于输入、无来源、括号内定位、自动补全的守卫）与
`textops` 的 2 条（删选区/删前一个字符、按字形簇删）。
全工作区 **614** 项测试全绿、clippy 零自有警告、生成器幂等、
探针已移除（`git diff | grep -c PROBE` = 0）、冒烟存活且 stdout/stderr 各 0 字节。
**截了应用窗口核对**：在 `foo fooBar food` 的光标后按 ⌥⎋，弹出三行候选、
首行高亮、贴在光标下方。
行数 41,117 → 41,805（`completion.rs` 361）。

### 8.21 警告检查器按上游重排成两栏 —— 以及一个会写坏文件的保存路径 ✅

`Inspector ▸ Warnings` 在 SubText 里是**一列自己编的提示文字**（混用换行符、U+FFFD、
行尾空白、编不出来的 BOM）。上游不是一列文字，是 **`VSplitView` 的两个面板**，
各带标题、一句状态和一个**表格**。

#### 上游的两个面板

```
Incompatible Characters     Line │ Character │ Converted
  Found N incompatible characters. / No issues found.
Inconsistent Line Endings   Line │ Line Ending
  Found N line endings other than LF. / No issues found.
```

- 标题 12pt semibold 次级色；状态行在**没有问题时**才用次级色。
- 表格：`.tableStyle(.bordered)`、圆角 8、`controlSize(.small)`，行号列**右对齐**且用
  tabular 数字；点一行把编辑器选区放到那个位置并居中。
- 「不兼容字符」只在**编码不是 Unicode** 且文本非空时才有内容 —— 这是上游自己的守卫。

#### 「不兼容」的判定：有损往返 + 二分

`String.charactersIncompatible(with:)`：把一段文字**有损编码**再**按同一编码解码**，
两者严格不等就是有问题；一段里如果不止一个字符，就**对半切开递归**，直到把出问题的
字符单独隔离出来。两种问题一条规则一起抓：编码装不下的字符，以及编码装得下但**写出去
会变成另一个字符**的（Shift-JIS 的 `\` → `＼`）。

二分是必须的：往返是编解码器调用，逐字符试在大文件上慢得不能接受，而绝大多数文档几乎
全是兼容字符。SubText 的扫描结果**按内容哈希记忆**（键是缓冲区哈希 + 编码），
所以每帧渲染不会重扫。

#### 顺带查出一个会写坏文件的 bug

`Document::save` → `FileEncoding::encode()` **丢掉了 `had_errors` 标志**。于是把一份带
emoji 的文档按 Shift-JIS 保存，磁盘上会**静默**出现 `&#65533;`（encoding_rs 对有损替换
写的是数字字符引用），而文件看起来保存成功了。上游的 `LossyEncodingError` 会拦住它。

现在保存路径会拒绝并报出上游那句话（*The document contains characters incompatible
with "Japanese (Shift JIS)"*）。上游给的是三个选择（*Save Available Text* / *Change
Encoding* / *Cancel*），SubText 做的是「取消」那一个，并把「改编码」指到状态栏的编码
选择器上 —— 记账里写明了「照原样保存」没做。

#### 记账的偏差

| 偏差 | 说明 |
| --- | --- |
| `Converted` 列显示的是 WHATWG 的替换 | encoding_rs 对有损替换写 `&#65533;`，Foundation 写 `?` / `＼`。**判定哪些字符不兼容两边的规则一样**，只是替换物不同 —— 与编码注册表「8 条是 WHATWG 超集近似」同源。 |
| 表格不能按列排序 | 上游的表头可点排序。这里用组件库里无状态的 `Table`（行数是「正在用遗留编码写的文档里的几个出问题的字符」，不需要虚拟化）。 |
| 表格横向滚动而不是压缩列 | `Table` 的单元格有最小宽度，三列塞不进窄检查器。上游的 SwiftUI `Table` 会压缩并截断。 |
| 点一行是滚入可见而不是居中 | 与大纲导航同一条：`EditorState` 没有居中接口。 |

验收：**9 条新测试**（623 = 614 + 9）——core 的 `warnings` 8 条（Unicode 编码与空文本
没有内容、编码装得下的文本没有内容、装不下的字符逐个列出、**连着的两个不会互相藏住**、
范围是字节偏移、不一致换行符的上游向量、CRLF 算一个、六种换行符都认）与 `document`
的 1 条（编不出来的文档不写盘、且磁盘上什么都没留下）。
全工作区 **623** 项测试全绿、clippy 零自有警告、生成器幂等、
探针已移除（`git diff | grep -c PROBE` = 0）、冒烟存活且 stdout/stderr 各 0 字节。
**截了应用窗口核对**：两个面板各带标题与状态行、两张表按列排好、
「Found 5 incompatible characters.」与「Found 1 line endings other than LF.」都对。
行数 41,805 → 42,382（`warnings.rs` 265）。

## 阶段 9 — 验证与发布 v0.2.0 ✅

| 任务 | 内容 | 状态 |
| --- | --- | --- |
| 9.1 | 全量测试 + clippy 零警告 | ✅（623 项 / 零警告） |
| 9.2 | 冒烟运行 + 打包 | ✅ 调试与 release 二进制各存活 7 秒、stdout/stderr 各 0 字节；`tools/package-macos.sh` 产出 `dist/SubText.app`（70 语法 / 14 主题 / plist ok / ad-hoc 签名）与 `dist/SubText-0.2.0-arm64.zip`，打包后的 bundle 再冒烟一次也存活。**§8.15 / §8.16 / §8.17 之后各重跑一遍**，同形（调试二进制存活 8–9 秒、输出为空）。**Windows / Linux 交叉检查未完成**，停在依赖的构建脚本上（缺 `llvm-rc` / `x86_64-linux-gnu-gcc`），见缺口表 |
| 9.3 | 更新 CHANGELOG、README、FEATURE-MATRIX | ✅ CHANGELOG 新增 `[0.2.0]`；README 的保真度表与测试数同步；FEATURE-MATRIX 的行数、编码三行、导航栏 / 状态栏 / 文稿检查器三行、统计同步 |
| 9.4 | 提交、打 `v0.2.0` 标签、重新打包 | ✅ 提交 `3795da1` 打标签后，§8.12–§8.21 依次落在其后。因为标签只在本地、尚未推送，每次都把它移到最新提交并按新源码重新打包，使标签、CHANGELOG 与 `dist/` 三者一致。**§8.15 落在 `26ac4df`、§8.16 落在 `b452c12`、§8.17 落在 `0466b04`、§8.18 落在 `dd2e2ba`、§8.19 落在 `e5b4a89`、§8.20 落在 `6c12abf`、§8.21 落在其后**，标签已移过去，release 二进制与 `dist/SubText.app` + `dist/SubText-0.2.0-arm64.zip` 都按它重建过。**推送前不要移动已发布的标签** —— 这里只是因为还没有远端 |

---

## 已知缺口（不阻塞，但需记账）

| 缺口 | 说明 |
| --- | --- |
| `cargo fmt` 未接入 | 全仓 20+ 个文件与 rustfmt 默认风格不一致（`actions!` 等宏调用的换行方式无法用 `rustfmt.toml` 对齐）。`menu.rs` / `keymap.rs` 是生成文件，要接入 fmt 得先让 `tools/generate_menu.py` 输出 rustfmt-clean 的代码（`actions.rs` 只被生成器**读取**，不写入）。当前门禁是 **clippy 零警告 + 测试全绿**，不含 fmt。 |
| Unicode 规范化的 3 个 Apple 形态 | `NFKC Case-Fold` / `Modified NFD` / `Modified NFC` 需要 `Default_Ignorable_Code_Point`、全量 case fold（`CaseFolding.txt`）与 Apple 的 HFS+ 表，`unicode-normalization` 0.1.25 都没有。菜单照列、触发时在状态栏说明（见 §8.9b）。 |
| 编码注册表与上游默认列表的残余差异 | 已逐条对齐（§8.9f）：上游 60 条默认列表里 **38 条照搬**——名字按 Foundation 的 `localizedName` 校正（12 条改名）、新补 1 条（Russian (DOS) / cp866）、把 4 对重复条目合并成 4 条（windows-1250、ISO 8859-2、8859-4、8859-9 各有一对被拆在两组里）、另有 6 条降级出菜单；**22 条如实缺席**（11 条 Mac OS codec：MacJapanese、MacChineseTrad/Simp、MacKorean、macArabic、macGreek、macHebrew、macCyrillic、macCentralEurRoman、macTurkish、macIcelandic；Latin-US (DOS) 的 cp437；NextStep Latin；ASCII；EUC-TW；5 条被超集 codec 覆盖的 DOS/Big5-E：DOSJapanese/DOSKorean/DOSChineseTrad/DOSChineseSimplif/Big5-E；以及 Unicode (UTF-16)/(UTF-32)）。菜单另有 **8 条是超集近似**：ISO Latin 1 → windows-1252、ISO Latin 5 → windows-1254、Thai (ISO 8859-11) → windows-874、两条 Shift JIS → cp932、GB 2312 → GBK、Korean (EUC) → cp949、Big 5 HKSCS → WHATWG Big 5（HKSCS 映射到真实码点而非 PUA）。名字说一套、codec 是另一套这件事无法消除——`encoding_rs` 只有 WHATWG 的 codec 集合——但每一项都在 §8.9f 的表里写明。 |
| 缺 `Unicode (UTF-16)` 与 `Unicode (UTF-32)` 两条菜单项 | 上游这两条的意思是「让 BOM 决定字节序」，写出时用哪个字节序由 Foundation 决定（跟平台原生字节序，不跨平台可复现）。SubText 的 `FileEncoding` 把字节序做成显式的（`utf-16le` / `utf-16be`），`Bom` 是文件的属性。要补这两条，先得给 `FileEncoding` 一个「未指定字节序」态并定义写出规则，是一个独立改动，不在 §8.9f 里猜。 |
| 编码列表编辑器的手势与上游不同 | 上游的列表是 SwiftUI 的 `List`，靠**拖拽**排序、靠 **Delete 键**删除；`gpui-component` 0.7.3 既没有拖拽重排，也没有能上报 Delete 键的列表。SubText 用一对箭头按钮与一个 Delete 按钮，能力相同（任何条目都能一次挪一格、任何可删条目都能删），手势不同。两个箭头**各自**判断能否移动：选区里只要有一行能走，那个方向的按钮就是活的，走到边上的那一行自己停住——上游的 `move(fromOffsets:toOffset:)` 对整段一起处理，这里对「选区同时含边界行与可动行」的情形略有不同。列表上方那句指示因此写成 *Move encodings up or down…* 而非上游的 *Drag encodings…*。另一条装饰性差异：对话框标题用 *Encoding List*（上游的 sheet 没有标题栏）。选择器里没有分隔线这条已在 §8.14 关掉（`pick_list_with_gaps`）。详见 §8.11。 |
| UI 集成测试不可用（`gpui-kit` 0.7.3 的 `test-support` 无法启用） | 组件库的 `#[gpui_kit::test]` + `TestWindowExt` 需要给 `gpui-kit` 打开 `test-support`。打开它会把整个 `gpui-fast` 家族拉进依赖图，而那一族把补丁版本钉死：`gpui-fast-macos` 要 `cocoa = "=0.26.0"`、`gpui-fast-web` 要 `unicode-properties = "=0.1.3"`，本项目依赖的 `gpui-pre-*` 0.3.9 拿的是 `cocoa 0.26.1` 与 `unicode-properties 0.1.4`，`^` 都兼容、`=` 都不兼容——**依赖解析直接失败**，连降级都过不去（降到 0.26.0 后下一个冲突是 `unicode-properties`）。`gpui-kit` 的 `test-support` 里写的是 `gpui-fast?/test-support`（`?` 表示「已启用才带上」），但实测它仍被激活，这是上游打包的问题，不是本仓能改的。因此交互层只能按本仓既有做法测：可测的规则全部抽成纯函数（§8.11 的 18 条编辑器测试就是这么来的），跨窗口与 overlay 那条路径用一次性冒烟钩子验证后还原（§8.11 结尾）。`gpui_kit::test` 模块本身也被 `#[cfg(feature = "test-support")]` 挡住，所以「只给子 crate 开 feature」这条绕法不通。 |
| 三个副窗口此前没有 Base `Root`（已修） | `gpui_kit::open_window` 会把视图包一层 `base::Root`，`cx.open_window` 不会。设置窗口、关于窗口、快速操作面板走的是后者，所以它们从来没有组件窗口状态，也就从来没有能力打开 overlay——只是此前都不开对话框，所以没暴露。§8.11 的冒烟探针第一次跑就崩在这里，三处已一并改成 `gpui_kit::open_window`（`Workspace` 因此多存一个 `settings_view: Option<WeakEntity<SettingsWindow>>`，因为窗口的根视图不再是 `SettingsWindow`）。**判据**：`grep -rn 'cx\.open_window' crates/` 应为空。 |
| Quick Actions 的行字形是替代品 | 上游给菜单命令画 SF Symbol `filemenu.and.selection`、给脚本画 AppleScript 字形；SubText 用组件库里最近的一对 `SquareMenu` / `SquareTerminal`，同一个方框。详见 §8.9e。 |
| `Script` 菜单的四处偏差 | 无修饰键行为（上游 Option 打开 / Option+Shift 显示，GPUI 菜单项不带修饰键状态，点击恒为运行）；无 tooltip（上游的 `"Option-click to open script in editor."`）；`tell application "CotEditor"` 无处可去（SubText 不暴露 scripting dictionary）；事件脚本（`CotEditorHandlers` 的 `documentOpened` / `documentSaved`）未实现。目录变化由 2 秒轮询代替 `NSFilePresenter`。详见 §8.9d。 |
| 警告检查器的 `Converted` 列显示 WHATWG 的替换物 | encoding_rs 对有损替换写 `&#65533;`，Foundation 写 `?` / `＼`。**哪些字符不兼容两边的规则一致**，只是替换物不同。 |
| 警告检查器的表格不能按列排序 | 上游的表头可点排序；SubText 用无状态的 `Table`，行数是几个出问题的字符，不需要排序也不需要虚拟化。 |
| 警告检查器的表格横向滚动 | `Table` 的单元格有最小宽度，三列塞不进窄检查器；上游的 SwiftUI `Table` 会压缩并截断。 |
| 有损保存只能取消，不能「照原样保存」 | 上游 `LossyEncodingError` 给 *Change Encoding* 与 *Cancel*，关闭文档时还多一个 *Save Available Text*。SubText 只做取消，并把改编码指到状态栏的编码选择器。 |
| 补全的候选列表没有虚拟化 | 上游是 AppKit 的弹出列表；SubText 用一列普通行放进 `max_h` + 滚动。行的数量由共享前缀的词数决定，通常很小，但一个字符的前缀在大文件里可以有很多行。 |
| 补全不判断 IME 合成态 | 上游 `performCompletion` 先问 `hasMarkedText()`，正在拼字时不弹列表。GPUI 的编辑器不暴露这个状态。 |
| 自动补全（*Suggest completions while typing*）未做 | 上游在插入文本后 250ms 防抖，并要求光标前是词字符、词长 ≥ 3、光标后不是字母数字。SubText 只做了 ⌥⎋ 这一半，那个开关在面板里仍是禁用的。 |
| 高级计数的胶囊不能拖动 | 上游的 `DraggableHostingView` 让用户把它拖到正文的任何地方，还能把它拖到窗口边上。SubText 固定在正文右上角 —— 也就是新开一颗时它出现的位置。 |
| 高级计数的胶囊没有 macOS 26 的玻璃效果 | 上游 `.glassEffect(in: .capsule)`。用主题的 `popover` 底色 + 描边 + 阴影，读起来是同一颗胶囊。 |
| 高级计数的规范化菜单只有 4 项 | 上游列 `standardForms`（5）+ `modifiedForms`（2）。其中 NFKC Case-Fold 与两个 HFS+ 形态是 SubText 没有的 Unicode 数据，`normalize` 对它们是拒绝而不是近似 —— 列出来只会让计数静默地错。Text ▸ Unicode Normalizations 的**菜单**仍然列全 7 项并说明原因。 |
| Inspect Character 的大字形不是衬线体 | 上游 `.fontDescriptor.withDesign(.serif)`、64pt。主题只有 `font_family` 与 `mono_font_family`，没有衬线一族，所以用界面字体画那个大字。 |
| Inspect Character 没有「闪一下选中范围」 | 上游在弹 popover 前还会 `showFindIndicator(for: selectedRange)` 把选中范围闪一次。GPUI 没有那个 API。 |
| `unicode_tables.rs` 的生成器需要 Swift | `tools/generate_unicode_tables.swift` 从 Swift 标准库读 `age` / `isDeprecated` / `isVariationSelector` —— 那正是上游读的那份数据，比手抄 `DerivedAge.txt` 可靠。代价是**重新生成只能在有 Swift 的机器上做**。生成的文件已入库，所以构建不需要 Swift；`cargo test` 里那条守卫负责发现「表与代码脱节」。 |
| 文档检查器没有 `Tags` 行 | **§8.16 重排后剩下的唯一一行缺口。** 上游从 `com.apple.metadata:_kMDItemUserTags` 扩展属性读标签，值是 **binary plist**：`PropertyListDecoder` 解成 `[String]`，每项 `"名字\n颜色序号"`（序号 0–7 → none/gray/green/purple/blue/yellow/red/orange）。Rust 侧要么写一个 bplist 解析器、要么加新依赖（`plist`），而 `xattr` 与那套调色板又都只有 macOS 有。这一轮没做，`Tags` 行也**没有画** —— 一行永远填不上的比没有这一行更糟。 |
| `Words` 走 UAX #29，不走 ICU 的分词 | 上游 `numberOfWords` 是 `enumerateSubstrings(.byWords, .localized)`，即 ICU 的**本地化**分词：除 UAX #29 的规则外还带词典（`こんにちは` 是 1 个词）与 locale。Rust 侧没有 ICU。规则与词典在「词之间写空格」的文字上完全一致（上游 `countWords` 的向量全过：`"Clarus says moof!"` 3、`"plain-text"` 2、`"!"` 0），只在 CJK 一类文字上分道：`こんにちは` 规则 5 / ICU 1，`漢字テスト` 规则 3 / ICU 2（实测见 §8.16）。ICU 的答案本身随 locale 变，所以没有「上游的那个数」。 |
| 检查器的 `Full Path` 是尾部截断、没有 tooltip、不可选 | 上游 `truncationMode(.middle)` 截中间，带 `help` tooltip，且 `.textSelection(.enabled)`。GPUI 只有尾部截断；`div` 没有 tooltip（只有 Button/Checkbox 一类有）；GPUI 的文本默认不可选。 |
| 检查器的 code point 没有描边框 | 上游把每个 code point 放进 `RoundedRectangle(cornerRadius: 3.5).strokeBorder` 的方框里；SubText 用空格连成一串。 |
| 检查器的四段没有展开动画 | 上游 `DisclosureGroup` 带 SwiftUI 的默认展开动画。组件库的 `Accordion` 用 `spring` + `MotionReveal` 做动画，但它的 chevron 在**后**、标签样式也不是上游那一套；这一轮用的是同一套基元里无样式的那一个（`AccordionTrigger`），动画一并舍去。 |
| 检查器的 `Created` 在部分平台为空 | `std::fs::Metadata::created()` 在 Linux 上没有 birthtime 时报错，那一格就是空的（上游的 `.creationDate` 在 APFS 上总有值）。 |
| 检查器的 `Owner` 在 Windows 上为空 | `getpwuid_r` 是 unix 的调用。`std::fs` 在 Windows 上没有「取拥有者名字」的接口，所以返回 `None` 而不是一个数字替身。 |
| 最近文档的条目标题不是 AppKit 的渲染 | 上游 `File ▸ Open Recent` 的条目由 `NSDocumentController` 自己画：这个构建既读不到它写什么标题，也没有那个钩子（见 §8.9g）。SubText 用文件名，与侧栏的最近文件行一致。数量（10）与「空历史时 `Clear Menu` 置灰」两条已实测对齐。 |
| `Straighten Quotes` 不随选区置灰 | 上游在空选区时禁用它（`validateUserInterfaceItem`，见 §8.9h）；GPUI 的菜单项只有固定 `disabled`，要跟着光标动就得每次选区变化重建 258 项菜单栏。SubText 保持可用、空选区时在状态栏说明，两种情况都不改文本。 |
| 大纲导航找不到条目时不置灰 | 同一条理由的第二次出现（见 §8.10）：上游按 `canSelectPreviousItem` / `canSelectNextItem` 决定置灰，SubText 保持可用、在状态栏说明方向。 |
| 大纲里 `title` 类条目在 `heading` 之后的层级 | 上游 `normalizedLevels` 有一条 `title` 特例：前一个非分隔项是 `heading` 时，`title` 落在它**下一层**（`lastNormalizedLevel + 1`），否则与前一项目同级。SubText 的 `indent_for` 一律给 `Level(0)`。**38 个内置语法里够不到这个分支**：`title` 只出现在 iCalendar（唯一一条规则，前一项也是 `title`）与 SVG（前一个非分隔项是 `value`，其缩进是捕获空白，走 `guard` 分支、不更新 `lastNormalizedLevel`），两者都取不到 `heading` 那一支。只有用户自写的、同时含 `heading` 与 `title` 规则的语法才会碰到。 |
| 大纲行只滚入可见，不居中 | 上游 `selectItem(id:)` 用 `centerSelectionInVisibleArea`（居中），SubText 的 `set_selection` 只滚入可见——`EditorState` 没有居中的公开接口。⌃⌘↑/⌃⌘↓ 那一侧上游也只是 `scrollRangeToVisible`，两边一致（见 §8.10）。 |
| `extract` 的去重是 SubText 自己的 | 上游**没有**去重：`removingDuplicateIDs` 只是兜底，防的是身份解析器把同一个 id 发两次，而它不会。SubText 合并「同跨度且同标签」的两条规则（见 §8.10），这是与上游不同的一处保留行为。 |
| 「生成表里每个命令都要有人在 `dispatch` 里认领」没有常设门禁 | 这条规则目前由一个仓库外的手工探针验证（直接 `import generate_menu` 复用选择器→动作名的真规则，再扫 `dispatch` 的字符串字面量）。要变成常设门禁，得先能静态读出 `dispatch` 认领的名字集合——在 Rust 测试里重写那条转换规则就是 8.9g 那次误报的老路。 |
| Windows / Linux 未编译验证（主机缺交叉工具链，不是代码问题） | 两个目标都试过 `cargo check --workspace --target …`，都在**依赖的构建脚本**上停下，还没轮到本仓的代码：<br>· `x86_64-pc-windows-msvc`：`gpui-pre` 的 build.rs 走 `embed_resource` 编译 `gpui.rc`，要 `llvm-rc`，本机没有（`xcrun -f llvm-rc` 与 `which llvm-rc` 都为空）→ `NotAttempted("llvm-rc")`。补法：`brew install llvm` 后把 `$(brew --prefix llvm)/bin` 放进 `PATH`。<br>· `x86_64-unknown-linux-gnu`：`cc-rs` 找不到 `x86_64-linux-gnu-gcc`。补法：装一套 Linux 交叉工具链（如 `messense/macos-cross-toolchains`）并设 `CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER`。<br>两者都需要在**用户机器上装东西**，所以没有擅自做；装好后这两条命令就是判据。已装的目标：`aarch64-apple-darwin` / `x86_64-apple-darwin` / `x86_64-pc-windows-msvc` / `x86_64-unknown-linux-gnu` / `wasm32-unknown-unknown`。 |
| 拖动分栏的**手势**没测过（宽度本身测过） | §8.13 量的是「面板解析后的宽度」，四种显隐组合都对。但「按住分隔线拖」这个动作没有 UI 集成测试可用（见上一条），所以拖拽的手感（命中区域、光标、拖动中的跟手程度）只有靠跑一次真机才知道。上游的 `NSSplitView` 命中区域比可视分隔线宽，`gpui-base` 的 `resize_handle` 也是（`resize_handle.rs` 的 hit area），但两者没有对比过。 |
| 冒烟测试的非 GUI 环境崩溃 | 10-14 23:34 有一份 `did_finish_launching` 的 SIGABRT 报告，发生在由 agent shell 以后台角色启动时。3 次复现未出现，未定位。 |

