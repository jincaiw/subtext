# 更新日志

本文件记录 SubText 的重要变更。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

---

## [0.2.0] — 2026-10-15

把 v0.1.0 里「菜单项保留、触发时在状态栏说明原因」的功能大部分真正做出来，
并把 UI 对齐 GPUI Kit 的设计规范。菜单仍是 258 项、快捷键仍是 72 条。

### 新增

**补上的功能（原来只有占位提示）**

- 分屏编辑器（`Display ▸ Split Editor` 三项），每个分屏独立撤销栈与光标。
- 文件夹查找：正则/字面量、区分大小写、包含隐藏文件、文件类型过滤、作用域
  （含保存的命名作用域）、后台扫描 + 进度、结果树。
- 多重替换：可编辑的规则表、TSV 导入导出模型、正则/忽略大小写/整词。
- 按模式排序（整行 / 分隔字段 / 正则三种排序键 + 五种开关 + 样例预览）。
- 编辑颜色代码（11 种记法，与 `ColorPicker` 联动）。
- 选区扩展：选择包围符号、按行拆分选区。
- 代码片段与文件拖放：设置窗口的 Snippets 面板（两个页签）、插入片段子菜单、
  `<<<SELECTION>>>` / `<<<CURSOR>>>` 令牌、拖放文件的 11 个令牌与五种图片格式。
- 快速操作（⌘K）：子序列匹配菜单路径、浮动面板、命令来源包含运行时条目。
- `Edit ▸ Straighten Quotes`（`[‘’‚‛]`→`'`、`[“”„‟]`→`"`）。
- `Find ▸ Select Previous / Next Outline Item`（⌃⌘↑ / ⌃⌘↓），落点是条目匹配的**范围**。
- **编码优先级列表编辑**（`Format ▸ Text Encoding ▸ Customize Encodings List…`
  与设置窗口 Format 面板的 *Edit List…*）：增删改序、Add 菜单、Restore Defaults，
  那份列表同时驱动编码菜单与打开文件时的探测顺序。

**运行时填充的菜单**

原来 11 个空占位子菜单里的 7 个现在是运行时构建的：`Format ▸ Line Endings` /
`Text Encoding` / `Syntax` / `Theme`、`Text ▸ Unicode Normalizations`、
`Text ▸ Insert Snippet`、顶层 `Script`，外加 `File ▸ Open Recent` 的最近文档列表。

**大纲改用 `Tree`**

可折叠层级、子序列过滤（`gt` 能找到 "Guten Tag"）、光标跟随、条目范围导航。

**设置窗口**

从 5 个面板扩到上游的 9 个（General / Appearance / Window / Edit / Mode / Format /
Snippets / Shortcuts / Integration），标签、顺序与分组逐项对齐。

### 变更

- **编码自动探测与上游同序**：BOM → 文件里的编码声明（可关闭）→ 按用户的优先级列表
  逐个试 → 默认编码。原来的统计猜测（`chardetng`）删掉了 —— 上游没有这一步，
  猜测会把「按用户排的顺序」变成「按统计概率」。
- 设置窗口的默认编码选择器跟着用户的优先级列表走。
- `LineEnding::ALL` 顺序修正为上游 `allCases` 的 `LF, CR, CRLF, NEL, LS, PS`。
- 编码注册表与上游默认列表逐条对齐（38 条照搬 / 22 条如实缺席 / 8 条超集近似）。

### 修复

- **三个副窗口没有 Base `Root`**：设置窗口、关于窗口、快速操作面板此前用
  `cx.open_window` 而不是 `gpui_kit::open_window`，因此没有组件窗口状态，
  一开对话框就会崩。三处已改正，并加了一条判据（`grep -rn 'cx\.open_window' crates/` 应为空）。
- 编辑器容器的高度误用「随基准字号缩放」的写法，实际只有一行高，改为 `h_full()`。
- 上次记录里写错的框架 API 名（`gpui_kit::relative` 不存在，正确的是 `gpui_kit::rems`）。

### 已知限制

以下功能的菜单项**保留**，触发时在状态栏说明原因：

- **平台受限（macOS 专属）**：拼写检查、语法检查、语音朗读、AppleScript dictionary、
  系统替换服务（Replace Quotes / Dashes / Text）。
- **未实现**：打印与页面设置、工具栏自定义、书写方向（RTL）、
  Unicode 规范化的 3 个 Apple 形态（NFKC Case-Fold / Modified NFD / Modified NFC）、
  补全、显示 Unicode 输入面板、向上/下扩展列选择、共享、捐赠窗口、更新日志窗口。
- **与上游手势不同**：编码列表编辑器用箭头按钮代替拖拽、用 Delete 按钮代替 Delete 键
  （`gpui-component` 0.7.3 没有拖拽重排，也没有能上报 Delete 键的列表）。

完整清单见 [docs/FEATURE-MATRIX.md](docs/FEATURE-MATRIX.md)，
逐条偏差见 [docs/PLAN.md](docs/PLAN.md) 的「已知缺口」表。

### 跨平台

- 已在 `aarch64-apple-darwin` 上完成编译、测试与运行验证。
- `x86_64-pc-windows-msvc` 与 `x86_64-unknown-linux-gnu` 的交叉检查**未完成**：
  两者都停在依赖的构建脚本上（Windows 需要 `llvm-rc`，Linux 需要
  `x86_64-linux-gnu-gcc`），本机没有这两个工具。不是代码问题，补法见 PLAN 的缺口表。

### 测试

**554** 项测试全部通过（v0.1.0 是 124 项）：

| Crate | 数量 |
| --- | --- |
| `subtext-core` | 304 |
| `subtext-app` | 213 |
| `subtext-syntax` | 29 |
| `subtext-theme` | 7 |
| doctest | 1 |
| **合计** | **554** |

`cargo clippy --workspace --all-targets` 零自有警告。

## [0.1.0] — 2026-10-14

首个版本。以 [GPUI Kit](https://github.com/longbridge/gpui-kit) 0.7.3 重写
[CotEditor](https://github.com/coteditor/CotEditor)，支持 macOS / Windows / Linux。

### 新增

**菜单与快捷键**

- 完整的应用菜单栏：258 个菜单项、28 个顶层菜单，结构与 CotEditor 逐项一致。
- 72 条快捷键，键位与 CotEditor 一致（`secondary` 在 macOS 上是 ⌘，
  在 Windows / Linux 上是 Ctrl）。
- 182 个类型化动作，分 10 个命名空间。
- `tools/generate_menu.py`：从 `resources/menu.json` 生成 `menu.rs` 与 `keymap.rs`，
  并自动附上测试守卫。

**文本处理**

- 38 个文本操作：切换/行内/块注释、取消注释、左右缩进、缩进转空格/制表符、
  5 种包围、修剪行尾空白、上下移行、排序、反转、打乱、删除重复行、复制行、
  删除行、合并行、大小写转换、全半角转换、URL 编解码、4 种 Unicode 规范化。
- 6 种换行符：LF / CRLF / CR / NEL / LS / PS。
- 6 种 BOM 的识别与保留：UTF-8 / UTF-16 LE,BE / UTF-32 LE,BE。
- 编码自动探测：UTF-8 有效性 → 统计猜测（chardetng）→ 用户默认编码。
- 字符统计：字符数、字数、行数、字节数。

**语法与主题**

- 直接复用 CotEditor 的 70 个 `.cotsyntax` bundle（`Info` / `Edit` / `Completion` /
  `Highlights` / `Regex/Highlights` / `Regex/Outlines`）。
- 直接复用 CotEditor 的 14 个 `.cottheme`。
- 高亮规则引擎复刻 CotEditor 的优先级与终止符语义。
- 大纲提取复刻 CotEditor 的 4 条隐式语义：多行锚点、空白压缩、
  缩进即层级、`separator` 产分隔项。

**界面**

- 主窗口：工具栏、标签栏、侧边栏、检查器、导航栏、状态栏、查找条。
- 侧边栏：文件浏览器、文件夹查找。
- 检查器：文稿信息、大纲、警告。
- 设置窗口：常规 / 外观 / 编辑器 / 格式 / 快捷键 五个面板。
- 关于窗口。
- 跳转到指定行对话框。

**查找与替换**

- 正则、区分大小写、全字匹配、循环查找、反向查找。
- 查找下一个/上一个、查找全部、选择全部匹配、全部替换、替换并查找下一个。
- 用所选内容查找 / 替换、高亮全部、移除高亮。

### 已知限制

以下功能的菜单项**保留**，触发时在状态栏说明原因：

- **平台受限（macOS 专属）**：拼写检查、语法检查、语音朗读、AppleScript。
- **未实现**：打印与页面设置、文件夹查找逻辑、多重替换、分屏编辑器、
  代码片段、快速操作、颜色代码编辑、书写方向（RTL）、选区扩展命令、
  按模式排序、工具栏自定义。

### 跨平台

- 已在 `aarch64-apple-darwin` 上完成编译、测试与运行验证。
- Windows / Linux 目标的编译验证待补。
- 等宽字体按平台选择：macOS `Menlo` / Windows `Consolas` / Linux `DejaVu Sans Mono`。

### 测试

124 项测试全部通过：

| Crate | 数量 |
| --- | --- |
| `subtext-core` | 56 |
| `subtext-syntax` | 29 |
| `subtext-theme` | 7 |
| `subtext-app` | 32 |
| **合计** | **124** |

`cargo clippy --workspace --all-targets` 零警告。

[0.1.0]: https://github.com/local/subtext/releases/tag/v0.1.0
