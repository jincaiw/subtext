# SubText 可行性评估报告

> 目标：以 [gpui-kit](https://github.com/longbridge/gpui-kit) 0.7.3 为 UI 框架，
> 重写 [CotEditor](https://github.com/coteditor/CotEditor)，做到**功能、桌面 UI/UX、
> 菜单、快捷键完全一致**，同时支持 macOS / Windows / Linux。
>
> 本文是动手之前做的评估：上游到底有什么、哪些能原样搬、哪些必须换实现、
> 哪些搬不动、风险在哪里。

---

## 1. 上游盘点

### 1.1 代码规模

| 指标 | 数值 |
| --- | --- |
| Swift 源文件 | 628 |
| 代码行数 | 约 72,128 行 |
| 应用主体 | `CotEditor/`（AppKit 应用） |
| 语法/主题/命令包 | `Packages/`（SPM 本地包，8 个） |
| 菜单定义 | `Main.storyboard`（258 个菜单项） |

### 1.2 关键子系统

| 子系统 | 上游实现 | 可移植性 |
| --- | --- | --- |
| 文本视图 | `NSTextView` 子类 + `NSLayoutManager` | ✗ 必须重写（GPUI 自带文本引擎） |
| 文档模型 | `NSDocument` / `NSDocumentController` | ✗ 必须重写 |
| 菜单 | `Main.storyboard` 的 `NSMenu` | ✓ **数据可提取**，结构可 1:1 还原 |
| 命令分发 | `@objc` selector + `NSResponder` 响应链 | ~ 需换成 GPUI action 体系 |
| 快捷键 | storyboard 的 `keyEquivalent` + `NSUserKeyEquivalents` | ✓ **数据可提取** |
| 语法定义 | `.cotsyntax` bundle（JSON + 正则） | ✓ **格式完全可复用** |
| 主题 | `.cottheme`（JSON） | ✓ **格式完全可复用** |
| 语法高亮 | `NSTextStorage` 属性 + 正则/tree-sitter | ~ 规则可复用，渲染需接 GPUI |
| 查找替换 | `NSTextFinder` + `NSSearchField` | ~ 逻辑可复用，UI 需重写 |
| 拼写/语音 | `NSSpellChecker` / `NSSpeechSynthesizer` | ✗ macOS 专属，无跨平台等价物 |
| 脚本 | AppleScript / OSA | ✗ 不存在于 GPUI |
| 打印 | `NSPrintOperation` | ✗ 需引入跨平台方案 |

### 1.3 结论

上游的**「数据」全部可复用，「行为」需要重写，「平台服务」需要降级**。
这一点决定了整个重写策略：把 CotEditor 当作规格说明（spec），而不是当作代码。

---

## 2. 框架评估：gpui-kit 0.7.3

### 2.1 它是什么

`gpui-kit` 是一个**单依赖**聚合层，`use gpui_kit::*;` 就等于拿到 GPUI：

```
gpui-kit 0.7.3
├── ::gpui::*              → 整层 glob（GPUI 的 trait 都在根下）
├── ::gpui_base    as base       → 无样式行为层（编辑器状态机在这里）
├── ::gpui_component as component → 样式组件层（Button/Dialog/Input/…）
├── ::gpui_kit_assets as assets  → 完整 Lucide 图标目录
└── ::gpui_platform as platform  → 平台抽象
```

底层是 Zed 的 `gpui-pre` 0.3.9。macOS 走 `gpui-pre-macos`（AppKit），
Windows 走 `gpui-pre-windows`（Win32），Linux 走 `gpui-pre-linux`（Wayland/X11）。

### 2.2 它能提供什么

| 需要的能力 | gpui-kit 的答案 | 评价 |
| --- | --- | --- |
| 窗口 / 事件循环 | `gpui_kit::application()` / `open_window()` | ✓ 够用 |
| 文本编辑控件 | `component::input::Editor` + `base::input::EditorState` | ✓ 够用，含行号/折行/缩进参考线 |
| 语法高亮接入点 | `InputHighlighter` trait + `InputHighlighterFactory` | ✓ **关键**：这正是接 `.cotsyntax` 的缝 |
| 代码折叠 | `FoldRange` + `InputHighlighter::fold_ranges` | ✓ 够用 |
| 菜单栏 | `Menu` / `MenuItem` / `cx.set_menus()` | ✓ 够用 |
| 快捷键 | `KeyBinding` + `cx.bind_keys()` | ~ 语法有坑，见 §4.1 |
| 组件库 | Button / Switch / Dialog / Notification / h_flex / v_flex | ✓ 够用 |
| 文件对话框 | `cx.prompt_for_paths()` / `prompt_for_new_path()` | ✓ 够用 |
| 剪贴板 | `cx.write_to_clipboard()` | ✓ 够用 |
| 图标 | `assets::IconName`（完整 Lucide） | ✓ 超出预期 |

### 2.3 它不提供什么

- **无原生菜单项勾选态**：`MenuItem::action(...).checked(bool)` 存在，但 `checked`
  由框架绘制，动态刷新需要在重建菜单时重新 `set_menus`。
- **无 NSTextFinder 等价物**：查找条要自己搭。
- **无打印**。
- **无 AppleScript / OSA**。
- **无 `NSSpellChecker`**：拼写检查、语法检查、语音合成在 Windows/Linux 上无等价物。

---

## 3. 可复用资产评估

这是整个项目里**最省力的部分**，也是保真度的来源。

### 3.1 `.cotsyntax`（语法定义）

70 个 bundle，每个是目录：

```
Rust.cotsyntax/
├── Info.json                 # 名称、扩展名、UTI、行注释符、缩进规则…
├── Edit.json                 # 自动补全括号、补全候选
├── Completion.json           # 补全词表
├── Highlights.json           # 高亮规则（旧格式）
├── Regex/
│   ├── Highlights.json       # 高亮规则（新格式，优先）
│   └── Outlines.json         # 大纲规则
```

**结论：格式是纯 JSON + ICU 正则，Rust 侧可 100% 解析。**
唯一需要移植的不是数据而是**语义**，见 §4.2。

### 3.2 `.cottheme`（主题）

14 个文件，17 个颜色键（`background` / `text` / `selection` / `insertion_point` /
`invisibles` / `line_highlight` / `comments` / `keywords` / …）。
格式是 JSON，颜色是 `#RRGGBB` 或 `#RRGGBBAA`。
**结论：可 100% 复用，且优先级顺序（用户目录 > 内置）也照搬。**

### 3.3 `menu.json`（菜单表）

从 `Main.storyboard` 提取的 258 条记录：

```json
{ "path": "File/Save As…", "title": "Save As…",
  "mods": ["command", "shift"], "key": "S", "action": "saveDocumentAs:" }
```

| 指标 | 数值 |
| --- | --- |
| 总条目 | 258 |
| 不同菜单路径 | 28 |
| 有 `action` 的 | 168 |
| 分隔符（`title == "-"`） | 52 |

**结论：菜单结构可以逐项生成，不需要手工誊写。**

---

## 4. 风险清单（按严重度排序）

### 4.1 【高】GPUI 修饰键只认 `ctrl`，不认 `control`

`gpui-pre-0.3.9/src/platform/keystroke.rs:120` 的 `Keystroke::parse` 只接受
`ctrl` / `alt` / `shift` / `fn` / `secondary` / `cmd` / `super` / `win`。

而 Cocoa 的 storyboard 里写的是 `control`。若直接照搬：

- **编译期完全静默** —— 没有任何错误、没有任何警告；
- **运行期瞬间 panic** —— `KeyBinding::new` 内部 `unwrap()`
  （`keymap/binding.rs:44`），抛 `InvalidKeystrokeError { keystroke: "control-shift-up" }`；
- 应用**启动即死**，且栈回溯指向框架而非业务代码。

**影响面**：72 条快捷键中 13 条受影响。
**缓解**：生成器做 `control → ctrl` 映射；并加一条 `every_binding_parses`
测试，把运行期 panic 前移成编译期测试失败。

### 4.2 【高】大纲提取有四个隐式语义，且 Cocoa/Rust 正则行为不一致

`Packages/Syntax/Sources/SyntaxParsers/RegexParser/OutlineExtractor.swift` 里有四条
只写在代码里、没写在文档里的语义：

1. `NSRegularExpression.Options = .anchorsMatchLines` —— `^` / `$` 绑定行首行尾；
2. 标题要经 `replacing(/(\S)\s+/) { "\($0.1) " }` **压缩连续空白**；
3. 用 `/^(?<indent>\s*)(?<title>.+)$/` 从**展开后的标题**读出嵌套缩进；
4. `kind == .separator` 时产出分隔项，而不是一个标签。

第 2 条与第 4 条容易漏。真正棘手的是替换模板：

`resources/syntaxes/Rust.cotsyntax/Regex/Outlines.json` 里写的是 `"$1struct $2"`。
**Cocoa 把 `$1struct` 解析为「组 1 + 字面量 struct」**，
**而 Rust `regex` crate 把 `$1struct` 解析为「名为 `1struct` 的组」并丢弃**。

同时 Rust `regex` **不支持 lookaround**，所以不能靠前后视断言绕过。

**缓解**：写一个手写扫描器 `brace_group_references()`，把 `$1struct` 改写成
`${1}struct` 再交给 `Captures::expand`。

### 4.3 【中】菜单项与动作必须一一对应，否则置灰

GPUI 的 `App::is_action_available` 会把**没有任何 handler 的菜单项自动置灰**。
CotEditor 有 13 个 Find 菜单项共用同一个 selector `performTextFinderAction:`，
靠 `NSTextFinderAction` 的 `tag` 区分——而 `menu.json` 里**没有 `tag` 列**。

后果：这些项会集体变灰，或集体映射到同一个动作。

**缓解**：用标题 → 动作的映射表把 `tag` 的歧义补回来
（`tools/generate_menu.py` 的 `TEXT_FINDER_ACTIONS`）。顺带修好了 3 个原本会置灰的死项。

### 4.4 【中】GPUI 重入约束

`entity_map.rs:232`：`panic!("cannot {operation} {entity_type} while it is already being updated")`。
视图自身的事件 handler 运行在该视图的 `update` 内部，因此 handler 里再去
`update` 同一个实体就会 panic。

**缓解**：用 `window.defer(cx, move |window, cx| …)` 把工作推到当前 update 之后。

### 4.5 【中】平台专属能力无法跨平台

| 能力 | macOS | Windows / Linux |
| --- | --- | --- |
| 拼写检查 | `NSSpellChecker` | 无 |
| 语音朗读 | `NSSpeechSynthesizer` | 无 |
| AppleScript | OSA | 无 |
| 打印 | `NSPrintOperation` | 需自建 |
| 服务菜单 | `NSMenu` Services | 无 |

**缓解**：这些动作**保留在菜单里**（保真度要求菜单一致），触发时在状态栏说明
「该能力在本平台不可用」，而不是静默失败。

### 4.6 【低】依赖体积与编译时间

GPUI 家族（`gpui-pre` / `gpui-pre-platform` / `resvg` / `rustybuzz` / `taffy` /
`ttf-parser` / `ropey` / `regex`）即使在 debug 下也需要优化，否则编辑器不可用。
已在 `Cargo.toml` 的 `[profile.dev.package]` 里统一设 `opt-level = 3`。

---

## 5. 结论与建议

| 判断 | 结论 |
| --- | --- |
| 是否可行 | **可行**。数据资产全部可复用，行为层需要重写但规模可控。 |
| 保真度上限 | 菜单 / 快捷键 / 语法 / 主题可达 **100% 结构一致**；平台专属能力受限于框架。 |
| 最大风险 | §4.1 的修饰键陷阱（编译静默、运行即死）与 §4.2 的大纲语义。 |
| 建议路线 | 先固化数据（菜单表、快捷键表、语法/主题解析），再搭 UI 骨架，最后填行为。 |

详见 [PLAN.md](PLAN.md) 与 [ARCHITECTURE.md](ARCHITECTURE.md)。
