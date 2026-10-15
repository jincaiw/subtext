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
- **侧栏与检查器可以拖动改宽**，宽度会记住：上游是 `NSSplitViewController`，
  两条分隔线都能拖且位置自动保存，现在 SubText 也是。侧栏 11–24 rem、检查器 12–26 rem，
  文档区吃掉富余。
- **默认编码是一个「文件编码」**：`Format` 面板的 *Default text encoding* 选的是
  codec **加** 是否写字节序标记，选择器里因此有了 *Unicode (UTF-8) with BOM*
  （紧跟 UTF-8），列表的分隔符画成横线。新建文档带上所选编码的标记 ——
  包括 UTF-16 / UTF-32 本来就该有的那一个。
- **窗口框架按上游的层次重排**：上游的 `WindowContentViewController` 是
  「侧栏 / 内容 / 检查器」三栏，**状态栏挂在内容项上**、**导航栏挂在每个编辑分屏里**。
  SubText 此前把两条栏都放在窗口底部，因此状态栏横跨侧栏与检查器、导航栏则是一条
  与上游无关的「语法 — 换行符」文字。现在：
  - 状态栏属于内容列，到侧栏边缘为止，内容是上游的那一行：不可编辑徽标 ·
    Lines / Characters / Words / Location / Line / Column · 文件大小 · 编码选择器 ·
    换行符选择器。六个计数各由一个开关控制（Window 面板的 *Status Bar* 一节），
    有选区时显示 `entire (selected)`。
  - 导航栏每个分屏一条：关闭分屏 · 分隔线 · 上一个 / 下一个 · 大纲菜单 ·
    分屏按钮（右键改方向）。关闭按钮在只有一个分屏时不可见，上一个/下一个的
    箭头随分屏方向换（竖直分屏用左右箭头）。
- **编码切换会问「转换还是重解释」**：`Format ▸ Text Encoding` 的菜单项与状态栏的
  编码选择器现在走同一个动作，并且与上游一样先弹
  *Convert* / *Reinterpret* / *Cancel* —— 文档没有文件或文本为空时直接改。
- **文档检查器按上游重排成四段**：原来是 SubText 自己编的十二行平铺列表，现在是
  `DocumentInspectorView` 的四个可折叠段 —— **File**（Created / Modified / Size /
  Permissions / Owner / Full Path，末行带 *Show in Finder*）、**Text Settings**
  （Encoding / Line Endings / Mode）、**Count**（Lines / Characters / Words /
  Location / Line / Column）、**Character**（选中单个字符时给 code point，
  单标量再给 Name / Block / Category；没有选区时写 *Not selected*）。
  `Count` 的三对计数与状态栏是同一个来源，所以两处不会给出不同的数。
- **Unicode 数据补真**：区块表从 72 条「大致会遇到的」补成上游那份 **346 条**
  （Unicode 17.0，未分配返回「无」而不是自造的 `No Block`）；`General_Category`
  从 `char` 谓词拼出来的近似换成 `unicode-properties` 的真表，30 个类别与上游的
  长名/短名逐条对齐（`Uppercase Letter (Lu)`）；控制字符补上 ASCII / ISO 6429 的名字
  （`NULL`、`NEXT LINE`…）。
- **文件属性**：Created / Modified / Size / Permissions（`644 (-rw-r--r--)`，
  含 setuid/setgid/sticky）/ Owner，日期按本机时区格式化。
- **`Icon::rotate` 让分屏方向回到「一个符号转 -90°」**，与上游的
  `.rotationEffect(.degrees(isVertical ? -90 : 0))` 同形。
- **Inspect Character 改成锚定选中字符的 popover**：原来是模态对话框加十行平铺的
  键值，现在是上游那样 —— 贴在字符上方（装不下翻到下方）、点外面或按 Escape 关掉、
  选区不是一个字符就不弹。内容也按 `CharacterInspectorView` 重排：一个大字形、
  名字（多标量时写 `<a letter consisting of N characters>`，无名时写 *Unknown*），
  然后是 Code Point（含代理对，deprecated 时带徽标）/ Block（点一下换成
  `Blocks.txt` 的名字）/ Category / Version；多标量的字符改成每个标量一个可折叠行。
  变体字符（`✔️`、`👍🏻`）名字后面括号里写 *Emoji Style* / *Skin Tone I-II*，
  控制字符的大字形画的是 Control Picture（`␀`、`␊`、`␡`）并改用次级色。
- **`tools/generate_unicode_tables.swift`**：面板要的 `age`（哪个 Unicode 版本收的）、
  `isDeprecated`、`isVariationSelector` 三个属性从 Swift 标准库生成
  （1787 / 8 / 4 段），生成的文件入库，构建不需要 Swift。
- **高级计数按上游重做**：`Text ▸ Advanced Character Count…` 原来是标题为
  *Document Statistics* 的静态对话框（九行固定的数字），现在是上游那个活计数器 ——
  先开一个 sheet 选选项（三个空白复选框 + *Unit* 菜单，选到 *Byte* 再给一个编码菜单，
  不是字形簇时再给一个规范化开关与菜单），按 **Start** 后正文右上角浮出一颗胶囊，
  显示计数（有选区就数选区，否则数整篇）并随打字自己更新；齿轮打开**同一份**选项面板，
  右键可以 *Copy* 那个数字或 *Stop Count*。字节单位编不出来时显示红三角 + *failed*。
  计数算法逐条照上游 `String.count(options:)`（忽略换行去 `\R`、忽略空白去
  `[\t\p{Zs}]`、连续空白折成一个、再规范化、再按单位数）。
- **`Text ▸ Surround Selection With ▸ Custom…`（⇧⌘L）**：文本菜单最后一个占位提示，
  现在是上游那个 sheet —— *Surround with:* 下面 `Begin:` 与 `End:` 两个输入框
  （`End` 的提示文字跟着 `Begin` 走），加一对 Cancel / OK。**end 留空表示「和 begin
  一样」**，所以 `"` 打一次就得到 `"…"`；**begin 为空时 OK 置灰**。用过的那一对会记住，
  下次打开就是它。

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
- **布局与界面令牌按 GPUI Kit 的设计规范复核**：应用层的间距一律 rem，
  只保留四处有理由的 `px`（窗口最小尺寸、`window.resize` 要的 `Pixels`、
  一物理像素的分隔线、用户设置的字号）；大纲的分类颜色收进命名的令牌模块，
  不再是散在分支里的字面量；滚动区域改成「外框负责滚动、内层负责内边距」，
  滚动条贴区域边缘而不是穿在内边距里。

### 修复

- **三个字符计数一直算错，而状态栏和检查器都在用它们**：
  - *Characters* 数的是 Unicode 标量而不是**扩展字形簇**（Swift 的 `String.count`），
    所以 `👍🏻` 报 2 而不是 1、`\r\n` 报 2 而不是 1；
  - *Lines* 把结尾的换行符算成了新开一行（`"a\nb c\n\n"` 报 4 而不是 3，
    空文档报 1 而不是 0）—— 上游的 `numberOfLines` 是「一行一个换行符，
    文本不以换行符结尾时再加一」；
  - *Words* 按空白与 CJK 标点切，所以 `plain-text` 报 1 而不是 2；
  - *Unicode Scalars* 干脆是 *Characters* 的副本。
  现在分别按扩展字形簇、上游的行数规则与 UAX #29 词边界算，并把上游
  `StringCountingTests.swift` 的四组向量搬成测试。**`Words` 与上游仍有一处差异**
  （ICU 的分词带词典），已记账。
- **五个内置包围命令包错了对象**：`Surround Selection With` 的 Single Quotes /
  Double Quotes / Parentheses / Braces / Brackets 一直包的是**行**，而上游包的是
  **选区**（`String.surround(in:begin:end:)`）。两个能看见的后果：在 `hello world`
  里只选中 `world` 按 ⇧⌘9，原来得到 `(hello world)`，现在是 `hello (world)`；
  没有选区只有一个光标时按 ⌘'，原来把整行包起来，现在是**在光标处插入一对引号**
  并把光标留在两半中间 —— 那本来就是这个快捷键的意思。
- **多标量字符的 Inspect Character 一打开就崩**：每个标量一行的展开状态是按标量序号
  取的，而选中时只建了 Block 名的那个向量，于是 `index out of bounds` 直接把进程
  abort。两个向量现在一起按标量数建好，取值也改成越界时当作收起。
- **状态栏的六个开关没有代码读**：Window 面板 *Status Bar* 一节的  *Show lines / characters / words / location / line / column* 六项，此前改了没有任何
  反应 —— 与上面那条 BOM 开关是同一类缺陷（界面在，行为不在）。状态栏按上游重写后
  六个开关各自生效，关掉的项不再出现。
- **状态位置量只认 `\n`**：`textops` 里算行列的函数数的是 `\n` 字节与标量，
  用 CR / NEL / LS / PS 写的文档行号永远是 1。状态栏要的是上游
  `StringCountingTests.swift` 那个问题（`Location` 0 基、`Line` 1 基、`Column` **0 基**，
  单位是扩展字形簇），所以新写了 `textops::status_position`，把上游的三组向量逐条
  搬成测试；原来那个函数留在原地服务 Go to Line。
- **「保存 UTF-8 时加字节序标记」这个开关是死的，而且上游没有这个开关**：
  SubText 的 Edit 面板里有一个 *Add a byte-order mark when saving as UTF-8*，
  但新建文档一律写 `Bom::None`，打开它没有任何效果；而 CotEditor 的 Edit 面板只有
  四节（Indentation / Whitespace / Comment / Content parse），根本没有这一段。
  `saveUTF8BOM` 在上游是 Format 面板默认编码的「标记那一半」，不是独立开关。
  现在它随默认编码一起设，新建文档真的带上标记（UTF-16 / UTF-32 的标记也一并修正），
  那个自造的开关删掉了。
- **设置文件里任何一个嵌套对象不完整，整份设置会被静默重置**：`Settings` 上有
  `#[serde(default)]`，但六个 `*Options` 结构都没有。容器级的那个只覆盖「顶层键缺失」，
  一个**存在但不完整**的嵌套对象会让反序列化整体失败，而读取路径是
  `unwrap_or_default()` —— 用户设过的每一项都被丢掉，文件按设计留在磁盘上，
  用户永远不知道为什么。这条同时是向前兼容的硬伤：给嵌套结构加字段就会让已有设置
  全部读不出来。六个结构各补 `#[serde(default)]`，并加两条测试盯住。
- **查找栏在最小窗口宽度下装不下**：两个定宽输入框加四个命令按钮的最小宽度是
  776 px，而窗口最小宽度是 720 px，窄窗下尾部按钮被裁掉且无法到达。
  输入框改成有下限的弹性宽度，计数格截断，主任务在每个允许的宽度上都在。
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

**602** 项测试全部通过（v0.1.0 是 124 项）：

| Crate | 数量 |
| --- | --- |
| `subtext-core` | 343 |
| `subtext-app` | 222 |
| `subtext-syntax` | 29 |
| `subtext-theme` | 7 |
| doctest | 1 |
| **合计** | **602** |

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
