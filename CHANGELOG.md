# 更新日志

本文件记录 SubText 的重要变更。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

---

## [Unreleased]

### 修复

- **Snippets 页的 `+` / `−` 现在是有边框的按钮**。它们用了 `ghost()`，于是只剩两个图标、像散落的
  符号；上游是两个带边框的 `NSButton`。

### 工程

- **截图走查入库**：`tools/peek-panel.sh <step> <窗口标题>` 截真窗口，`tools/peek-overlay.sh <step>`
  截对话框（`screencapture -l` 截不到对话框，它在窗口之上的独立层）。两份脚本都轮询 `pgrep`
  等上一个进程真的退出 —— `pkill` 返回不等于进程已消失，而截到旧窗口时三次不同的源码改动会产出
  逐字节相同的图。
- **`docs/UI-CHECKLIST.md`**：把这一系列走查修掉的 11 处缺陷归纳成 7 条规则，每条都指到具体的
  文件、提交与截图。改界面前读一遍，比每次靠截图发现快。

## [0.3.21] — 2026-10-29

### 修复

- **Shortcuts 页的 *Restore Defaults* 按钮之前在窗口外面**（只露出上边缘）：列表 `max_h(28rem)` 太高，
  把按钮行推出了窗口底边。降到 22 rem。
- **Window 页的 `Indent wrapped lines by … spaces` 被右边缘裁成 `spac`**：给这行加 `flex_wrap`，
  并把 stepper 的数值区从 4 rem 收到 2.5 rem。
- **Edit 页的 `Indent width:` 与 `Prefer using:` 不在同一列**：这一行没走 `field`，补上同样的
  12 rem 右对齐标签列。
- **Integration 页**：*Learn More…* 不再被拉成整行；说明文字里的 Markdown 反引号（`` `cot` ``）
  换成弯引号 —— 那是界面文案，不是 README。

### 文档

- 设置窗口九个页全部走查完毕（§22.20）。Snippets 的 `+` / `−` 没有按钮外观，没定位到构造位置，
  记进缺口表。

## [0.3.20] — 2026-10-29

### 修复

- **设置窗口不再开成全屏宽**。它此前只设了最小尺寸，于是继承主窗口的 1536 px，内容只用掉 530、右边
  三分之二空着。现在固定 760×580 居中。
- **设置页里两个按钮不再被拉成整行**（General 的 *Manage Warnings…*、Format 的 *Edit List…*）：
  按钮直接挂在 `v_flex` 上，列的交叉轴会 stretch 它。
- **Format 的编码列表改成左对齐**。行是 `Button`，而它的内部布局硬编码居中，一列编码名读起来像一列标题。

### 文档

- 设置窗口走查记进 §22.19；Mode 页的间距与换行**没修成**，连同「改了两处、截图逐字节相同、已回退」
  一起记进缺口表。

## [0.3.19] — 2026-10-29

### 修复

- **两个对话框的内容一直是被裁的**。`Custom Surround` 只显示 `Begin:`、`End:` 与**所有按钮**都在对话框
  之外；`Edit Color Code` 的 `Notation` 值被截成 `Hexade`、色值被截成 `#00`。原因是两者都没有 `footer`，
  而 sheet 自己画的按钮行在定宽对话框里放不下。现在都加了 `footer`（Cancel）、加宽到 32 rem，包围的两个
  字段改成上下排 —— 颜色码的标签列也因此有了位置。
- **`OK` 仍然不可见**：组件库这一版的 `DialogFooter` 只渲染一个 `DialogClose`，普通 `Button` 完全不渲染
  （四种写法都试过，见缺口表）。两个对话框现在用 **Enter 提交**、footer 的 Cancel 取消 —— 上游的
  `CustomSurroundView` 本来就是 `onSubmit`，Enter 是主路径。

### 工程

- **`SUBTEXT_PANEL_PEEK=<n>`**：打开第 n 个面板并停住，用于截图走查（`panel_sweep` 只回答「不崩」）。
- 走查方法本身也修了一处：`screencapture -l <windowid>` **截不到 GPUI 的对话框**（它在窗口之上的独立合成层），
  之前所有「sheet 已打开」的判断都只证明了进程没崩 —— 必须截全屏再裁窗口区域。

## [0.3.18] — 2026-10-29

### 新增

- **文稿检查器的四段现在有展开动画**，折叠到底时卸载（与上游的 `DisclosureGroup` 一样）。
  此前用的是无样式的 `AccordionTrigger`，把「chevron 位置不对」和「不要动画」一起舍掉了 ——
  动画其实在 `spring` / `MotionReveal` / `AccordionPanel` 三个基元上，与 trigger 用哪一个无关。

### 修复

- **检查器的 Full Path 改成中间省略**（上游是 `truncationMode(.middle)`），并且**悬停能看到全文、
  点击复制完整路径**。此前是尾部截断、没有 tooltip：`div` 挂不上 tooltip（`ManagedTooltipExt` 是
  组件库内部 trait），所以路径改成一个 ghost 按钮 —— 点击复制正好替代上游的
  `textSelection(.enabled)`。

## [0.3.17] — 2026-10-29

### 性能

- **状态栏的字符 / 词 / 行数改成缓存的**。它此前每次重绘都数一遍全文 —— 而重绘包括**每次光标
  移动和光标闪烁**。实测 1 MB 文本数一遍要 **24 ms**（release），所以大文件里移动光标会掉帧。
  现在计数挂在文档上，只在编辑时作废：1.11 MB 的文件打开时算一次，之后五次光标移动一次都没再算。

## [0.3.16] — 2026-10-29

### 修复

- **括号配对现在跟随光标移动**，不再只在打字后闪。上游在每次选区变化时判，SubText 把它挂在
  `InputEvent::Change` 上 —— 而矩阵里给的理由「编辑器不报纯光标移动」与 `subscribe_pane` 自己的
  注释相反。`observe(editor)` 一直都能收到光标移动（`sync_menu_gates` 就靠它），所以这只是接线
  漏了一处。顺手做了两层去重：先比选区（光标闪烁时不扫配对，`matching_symbol_pair` 对大文件是
  O(n)），再比配对（沿同一个 `foo(bar)` 移动不重复淡出）。

## [0.3.15] — 2026-10-29

### 新增

- **Help 菜单的帮助项现在真的打开文档**（仓库的 `docs/` 目录），不再只在状态栏说「文档在源码树
  里」。上游按锚点翻帮助书，纯可执行文件没有帮助书。
- **Edit ▸ Copy as Rich Text 现在真的复制富文本**。此前它提示「由文本引擎处理」，实际只拿到纯
  文本。现在把选区的语法着色写成剪贴板的 **RTF** 类型（同时带纯文本，粘到只收文本的地方也能用）：
  上游的 `copyWithStyle:` 写 `NSAttributedString`，而 GPUI 的剪贴板没有富文本，所以 RTF 由
  `subtext-core::richtext` 生成、经 `osascript` 写进 pasteboard。

### 文档

- **矩阵里 11 条「未实现」逐条回到上游核对**，十条里有五条的理由不准确，已改正：向上/下扩展
  列选择其实是**多光标**（不是视觉行模型）；Unicode 输入面板、共享是 AppKit 弹出层（⚠️ 而不是
  ⏳）；字体面板其实是 🔁（Format ▸ Font 已有字体族与字号）；事件脚本走的是 Apple event +
  带 `CotEditorHandlers` 的 app bundle；新建窗口是**设计决定**而不是能力缺失。

## [0.3.14] — 2026-10-29

### 新增

- **Help ▸ Create Issue Report 现在真的建一份报告文档**。此前它只在状态栏说一句「请到仓库提
  issue」。上游的做法是打开**一个新的未命名文档**，内容是一份 Markdown 模板，版本号、系统与语言
  已经填好，语法是 Markdown —— 版本和系统正是报告里最常写错的两项。SubText 现在照做。
- **官网 / 报告问题 / 发行说明三条命令现在真的打开网页**（`App::open_url`），不再只是提示。
  What's New 上游是独立窗口，离它最近的是同一版本的 release notes，所以那一条打开 release 页面。

### 工程

- `subtext-core` 新增 `issuereport`：`Environment`（版本 / 系统 / 语言，各自有取法与退路）
  与 `IssueReport::template`，版式由 5 条单元测试盯住。
- `subtext-core` 新增 `richtext`：`Run` → RTF，8 条单元测试盯颜色表顺序、转义、`\uN?` 的
  有符号码元、代理对与半点字号。
- 面板冒烟多一步：算上「创建报告」开的那个文档，20 → 21 个界面。

## [0.3.13] — 2026-10-29

### 新增

- **`Script ▸ Show Console` 真的打开了控制台**。它此前只在状态栏说一句「未实现」，但控制台是
  **脚本报错唯一的去处**：上游把脚本的标准错误交给控制台并把窗口提到前面，SubText 却塞进一行
  状态栏里，多行的 traceback 从第二行起就丢了。现在有一个 360×200 的窗口显示「时间戳 + 脚本名 +
  等宽正文」，右上 Clear 清空；日志是全局的，关掉窗口再打开还在。脚本失败时自动写入并打开。
- **大纲里选中的条目现在会居中显示**。上游的 `selectItem(id:)` 以
  `centerSelectionInVisibleArea` 收尾，SubText 只做到「滚入可见」。此前记的理由是
  「选中行的处理器拿不到 `Window`」—— 其实 `Context::observe_in` 就带 window。接上之后还要
  注意时机：光标几何来自上一帧的 prepaint，要把居中推迟**两帧**才能读到新光标（推迟一帧会把
  视图顶回第一行）。

### 工程

- **面板冒烟多一步**：控制台也算进 `SUBTEXT_PANEL_SWEEP`，19 → 20 个界面。

## [0.3.12] — 2026-10-29

### 新增

- **Format ▸ Center Selection in Visible Area 现在真的居中**。此前它只在状态栏说一句
  「Jumped to the selection」；`EditorState` 没有居中的接口，但 `prepaint_caret_geometry`
  给出光标在内容坐标里的位置、`set_scroll_offset` 可以设滚动偏移，两者相减就是答案。

### 工程

- **面板冒烟**：`SUBTEXT_PANEL_SWEEP=1` 让 debug 版在启动后依次打开 19 个界面（三个 sheet、
  三个检查器页、查找面板、九个设置页、关于窗口、侧栏两页）再退出。任何一处 panic 都会让退出码
  非 0 —— 这是第一个能覆盖「从没打开过的界面」的自动判据。

## [0.3.11] — 2026-10-29

### 新增

- **二十一种语言终于有语法着色了**（其中 19 种已生效）。Rust、Python、JavaScript、TypeScript、C、C++、Java、Go、
  Ruby、PHP、Swift、Kotlin、Scala、Lua、Bash、SQL、Makefile、C#、HTML、CSS 这些语法**没有**
  `Highlights.json` —— 它们的颜色是 CotEditor 放在 `queries/<Name>/highlights.scm` 里的
  tree-sitter 查询。现在照用上游的查询：冲突按「后声明的规则更具体」消解，按文档顺序上色，
  字符串里的 `//` 不会被当成注释。

### 修复

- **编辑器现在真的会画语法着色了**。文档面板渲染的是 `gpui-component` 的 `Editor`，而它
  每一帧都用组件主题重设一次编辑器样式，把应用设的颜色**在读到之前**覆盖掉 —— 于是
  SubText 至今没有任何语法着色，正则那 47 个语法也一样。现在把 `.cottheme` 的正文色、
  背景色与十个 token 的颜色投影进组件主题（`highlight::project_theme`），启动与切换主题时
  各做一次。
- **编辑器主题的其余部分也来自 `.cottheme` 了**：选区、光标、不可见字符、当前行高亮与行号槽
  此前仍是组件库自带的浅色主题，换一个深色主题时文字变了、选区还是那套蓝。现在这六项都按
  CotEditor 主题投影（缺的键仍按 CotEditor 的默认值补）。

## [0.3.9] — 2026-10-29

### 修复

- **大纲的每一行都按它自己语言的规矩写了**。上一版接上 tree-sitter 时只搬了七个格式器，
  其余走「名字 + 参数表」，于是 Go 的方法丢了接收者、Ruby 的丢了 `self.`、Scala 显示成
  `f(a: Int)` 而不是 `f(a)`、Swift 的方法只剩一个名字、C# 的析构函数丢了 `~`。现在十四个
  格式器全部移植：`(*T).M(a int)`、`self.m(a, b)`、`f(a, b)`、`m(a:b:)`、`~A()`、
  `IFoo.M()`、`G<T>(T a)`、`m<T>(a: T)`，以及 Swift 的 `MARK: - X` 折叠成 `X`。

## [0.3.8] — 2026-10-29

### 新增

- **21 种语言的大纲**。Markdown、Python、JavaScript、TypeScript、Rust、HTML、CSS、C、C++、
  Java、Go、Ruby、PHP、Swift、Kotlin、Scala、Lua、Bash、Makefile、C# 的大纲不是正则规则，
  而是 CotEditor 放在 `queries/<Name>/outline.scm` 里的 tree-sitter 查询 —— 此前 SubText
  只读 `Regex/Outlines.json`，这些语言的大纲面板**一直是空的**。现在照用上游的查询与语法，
  函数行带参数表（`main(int argc, char **argv)`）、HTML 取文本而不是标签、Markdown 去掉
  `#`。跳级标题与 `title` 的层级也按上游的 `normalizedLevels` 归一化了。

### 修复

- **`.md` 文件不再以 Plain Text 打开**。`SyntaxKind` 的取值与上游不一致（写成了
  `code`/`markup`/`none`，上游只有 `general`/`code`），于是七个说 `"kind": "general"` 的语法
  整份 `Info.json` 反序列化失败；`read_json` 把解析失败当成「文件不存在」，这些语法因此
  丢了 fileMap —— Markdown、LaTeX、BibTeX、BBCode、Plain Text 的扩展名映射全部失效。
- 大纲不再合并「同跨度同标签」的两条规则 —— 上游不去重，那是 SubText 自己的规则。

## [0.3.7] — 2026-10-24

### 修复

- **ISO Latin 1 / Latin 5 / Thai (ISO 8859-11) 现在是严格实现**。此前这三条借用了
  Windows 代码页（1252 / 1254 / 874），而它们在 `0x80`–`0x9F` 上与标准不同 ——
  一份 Latin-1 文件里的 `0x80` 会显示成 `€` 而不是 U+0080。现在按 Foundation 实测的
  字节表解码与编码，与 CotEditor 一致；Thai 还补上了 TIS-620 留的八个空洞。

## [0.3.6] — 2026-10-19

### 工程

- **全仓接入 `cargo fmt`**：49 个文件一次性格式化，CI 把 `cargo fmt --all --check`
  作为第一道门禁。生成菜单的两个文件由生成器自己格式化，所以重新生成不会把树弄脏。

## [0.3.5] — 2026-10-19

### 新增

- **编码菜单补上 *Unicode (UTF-16)* 与 *Unicode (UTF-32)***：这两条按「让 BOM 决定
  字节序」工作，写出时带本机字节序的标记，读入时由标记决定字节序。

### 修复

- **宽 Unicode 编码不再多写字节序标记**：*UTF-16LE/BE*、*UTF-32LE/BE* 这四条此前都
  带着标记写，比 CotEditor 多两个字节；现在按 Foundation 的行为不写（只有「未指定
  字节序」的两条带标记）。
- **NFKC Case-Fold 现在真的会折叠**。此前这一项在菜单里，点了只在状态栏说「需要
  CaseFolding.txt」——现在那张表和 `Default_Ignorable_Code_Point` 都由 Foundation 生成，
  `Straße` 变成 `strasse`、`ﬁ` 变成 `fi`，与 CotEditor 逐字符一致（400 条随机语料
  对拍过）。
- **关闭文档时选 *Save* 现在真的会保存并关闭**。此前对未保存过的新文档，这个按钮
  什么都不写（只在状态栏留一句 `no path`）；对已命名的文档，写的是**前台**那个标签
  而不是被关闭的那个。保存与关闭现在都按文档身份走：新文档先要路径、拿到路径后写盘，
  写完才关标签；在保存面板里取消就什么都不发生，标签留着。

## [0.3.4] — 2026-10-19

### 新增

- **Inspect Character 的大字形是衬线体**：与 CotEditor 一样用系统衬线（macOS 的
  *New York*、Windows 的 Georgia、其它平台的 DejaVu Serif），64pt。
- ***Find All* 的下拉里可以 *Select All***：一次把所有匹配选中（编辑器的多重选择），
  状态栏报出选了几个。此前那一项是灰的。
- **检查器里的 code point 有了描边框**：每个码位一个小圆角方框、等宽数字，
  选中多个标量时自动换行——与 CotEditor 的检查器同一形状。

### 工程

- **持续集成**：push 到 `main` 与 PR 时在 macOS 上跑全部测试与 clippy（`-D warnings`），
  并额外为 Linux 与 Windows 目标检查三个纯 Rust crate 的平台分支。
  第一次跑就抓到一个本地工具链看不见的 clippy 新 lint。

## [0.3.3] — 2026-10-19

### 新增

- **没选中文字时 *Straighten Quotes* 是灰的**：与 CotEditor 的菜单校验一致——选中一段
  文字它就会变亮。菜单栏只在选区「空 ↔ 非空」翻转时重建，不是每次光标移动。
- **查找标记会淡出**：命中后的黄色标记不再是「亮一下突然消失」，而是分五档淡完，
  总时长仍是 700 毫秒。
- **高级计数的胶囊可以拖动**：把它拖到正文的任何位置，停止计数后回到右上角。
  拖出窗口边缘不做（现由编辑区裁剪）。

## [0.3.2] — 2026-10-18

### 新增

- **配对括号会闪一下**：光标紧跟 `(` `[` `{` 时，配对的那个括号亮一下——与查找命中
  同一个黄色标记。嵌套会被正确计数，不配对的括号不亮。
- **增量搜索会把所有匹配标出来**：在查找字段里打字，正文里每一个匹配都带上浅灰底，
  关掉查找面板或清空查询就消失——与 macOS 的 *未强调选中背景色* 同一个意思。
- **警告检查器会在正文里标出问题字符**：打开 Warnings 面板时，每个不兼容字符
  都带上同样的灰底，切到别的面板就消失。
- **两个 HFS+ 规范化形态**（Text ▸ Unicode Normalizations ▸ *Modified NFD* / *Modified NFC*）：
  与 macOS 文件系统同名的那两种形态，此前只列在菜单里、触发时说明「没有表」。
  现在按 Apple 的实测行为实现：`Ω` `K` `Å` 与 CJK 兼容汉字不分解，组合符号仍规范排序，
  组合形态会跳过 Unicode 拒绝组合的那些字符。只有 *NFKC Case-Fold* 仍未做。
- **文档检查器的 `Tags` 行**：读文件的 `com.apple.metadata:_kMDItemUserTags`
  扩展属性（binary plist），在 *File* 段里逐条列出标签名与它的 Finder 颜色；
  没有标签时显示 `–`。文件没有被 Finder 打过标签、或系统不是 macOS，行照样在。
- **警告检查器的表可以排序**：点表头按那一列排，再点一次反向；活动的列表头带
  箭头。`Line` 按数字排（第 2 行在第 10 行之前），点一行仍会选中编辑器里的
  那个字符。不可见的字符（制表符、不换行空格等）在 *Character* 列显示成
  `U+XXXX`，不再是一格空白。列表长了在面板内滚动，不再压到下一节。

## [0.3.1] — 2026-10-18

### 新增

- **菜单命令的覆盖门禁**：新增 `menu_contract.rs`，随 `cargo test` 检查
  「生成表点名的每个命令 → bridge 里有绑定 → dispatch 里有分支」三段链条，
  并对解析结果设下限（表结构变形态时响亮失败，而不是无声地空差分）。
- **查找命中会闪一下**：*Find Next* / *Find Previous*、点 *Find All* 的结果行、
  文件夹查找跳转、*Inspect Character*，都会在匹配上闪一块黄色标记（上游
  `showFindIndicator`）后消失。标记画在文字背后，文字颜色不受影响。
- **增量搜索会移动光标**：停止输入 200 ms 后，光标移到当前匹配并在那里闪一下
  —— 此前只更新匹配计数，光标不动。`Find All` 表仍不受增量搜索影响。
- **自动补全（*Suggest completions while typing*）**：打字停顿 250 ms 后弹出候选
  列表，按上游 `performCompletion` 的四条守卫——选区为空、词长 ≥ 3、光标前不是
  空白、光标后不是字母数字。接受或 Esc 后不会再弹（上游 `insertCompletion`
  不重新装弹）。Mode 面板的 *Suggest completions while typing* 开关由此生效；
  *Standard words* 改为禁用并写明理由（AppKit 的系统词表 Rust 侧够不着）。

## [0.3.0] — 2026-10-17

### 新增

- **Find All 的分体下拉**：主体点击查找，caret 菜单给 *Highlight All* 与
  *Select All*（后者在编辑器支持多选前禁用）——上游 `Menu(primaryAction:)` 的形态。
- **查找面板记住窗口位置与大小**：关掉再 ⌘F，面板回到上次所在的帧
  （上游 `windowFrameAutosaveName`）。
- **关闭有未保存改动的文档会先询问**：上游 `DocumentClosingAlert` 的
  *Save* / *Don't Save* / *Cancel*（Don't Save 为破坏性样式），文案逐句对齐；
  有损时 *Save* 变 *Save Available Text* 并在正文写明不兼容字符将被替换或删除。
  空文档与干净文档不打扰。
- **有损保存的三选**：按 Shift JIS 等编码保存遇到不兼容字符时，弹
  *Save Available Text* / *Review Incompatible Characters* / *Cancel* ——
  第一项为本会话允许有损写盘（上游 `allowsLossySaving`），第二项打开
  Warnings 检查器。此前只有「取消」一条路。

## [0.2.0] — 2026-10-15

把 v0.1.0 里「菜单项保留、触发时在状态栏说明原因」的功能大部分真正做出来，
并把 UI 对齐 GPUI Kit 的设计规范。菜单仍是 258 项；快捷键从 72 条变成 75 条
（三条控制字符键位此前被生成器读成了空串）。

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
- **查找面板改成一个独立的浮动窗口**：`⌘F` 原来在文档窗口顶上开一条横条；上游的
  `FindPanelController` 是一个 `NSPanel` —— 标题 *Find & Replace* 的浮动工具窗口。
  现在也是：两个 **44pt 高、圆角描边**的字段（Find / Replace with）、一行三个复选框
  （Regular Expression / Ignore Case / **In Selection**）、
  `[Find All] [Replace All] … [Replace] [◀] [▶]` 的按钮行。窗口 520×190 居中打开，
  用窗口自己的关闭键。`⌘E` / `⇧⌘E` 顺带把面板带出来。
  结果消息按上游放在**查找字段的尾随边缘**上（和清空按钮一起，字段为空时不画），
  有当前匹配时是 `1/4`、否则是 `4 matches` —— 上游 `positionMessage` 的
  `defaultValue` 就是 `"\(index)/\(self.count)"`，不是「1 of 4」。
- **`Find All` 的结果列表**：面板中间那一段 —— 上游三段纵向 split 的第二段，
  默认收起、`Find All` 找到东西才打开并占 200 点。现在也有：`[⌃]`
  折叠按钮加粗体的 `Found N matches in "文件名."`、一行 `Find text: 查询`、
  一个两列表格（`Line` 右对齐窄栏 / `Matched Text` 尾截断），右键出
  *Text Size ▸ Bigger / Smaller / Reset to Default*。打开与折叠时面板窗口跟着长高、
  收回。行号是**六种**换行符的口径（与状态栏一致），表头的头部截断按**字形簇**
  回退 16 个而不是按标量。行可以像上游一样**多选**（⌘ 切换、⇧ 范围），
  `⌘C` 把选中行按显示顺序放到剪贴板；只选一行时编辑器选区跟着移过去。
  `Line` 表头可以点按排序（升/降），排序重排的是同一份行数组，
  复制顺序随之。正则捕获时每个捕获组按上游 `textHighlighterColor.decompose`
  的同一算法（实测默认强调色 α0.4 均分色相）画背景色，重叠处最细的组赢 ——
  group 0（整匹配）也画。菜单栏的 *Bigger/Smaller/Reset Font* 与 *Copy*
  在表格持有键盘焦点时作用于表格，其余时候照旧归编辑器。
- **查找面板的两个历史菜单**：每个字段左上角一个图标按钮（放大镜 / 铅笔），
  下拉列出最近 20 条查找 / 替换串（去重后追加尾部、空串不记），超过 64 字符的
  条目显示截断，点击把那一串填回字段而不触发搜索；尾部的 *Clear Recent …*
  在没有历史时禁用。文字区按上游 `FindPanelTextClipView` 的 32/24 点内边距
  避开图标与清空按钮。替换动作同时把查找与替换串记入历史，并在替换字段的
  尾随边缘显示 `N replaced`，替换文本一改就消失。
- **正则速查弹层**：*Regular Expression* 开关旁的 ？ 按钮弹出上游那份
  *Basic Regular Expression Syntax* 速查 —— 五组定义（字符 / 锚点 / 量词 /
  扩展组 / 反向引用）逐条照搬，两列布局，尾部是通向 ICU 规范的外链。
  弹层在面板窗口内限高滚动（上游的可拖出 popover 能浮在窗口外，GPUI 做不到）。
- **高级查找选项弹层**：字段行右端的 ⋯ 按钮弹出上游 `FindSettingsView` 的
  三节十个开关。其中六个真实生效 —— *Wrap search around* 与
  *Match only whole words* 是原有设置，*Search incrementally* 是新能力
  （输入即搜，Find All 的结果表不受它影响），*Dot matches line separators*、
  *Anchors match lines* 与 *Unescape replacement text* 接进正则引擎
  （`^`/`$` 不再固定按行匹配，替换文本里的 `\n` 默认被解字）。另外四个
  （严格逐字符等价、忽略变音符、忽略全半角宽度、TR#29 词边界）是 regex
  引擎没有的能力：开关照列、禁用并在提示里说明原因。
- **新增 *In Selection***：只在选区里查找。它是把搜索的 **scope** 收窄，匹配仍然是
  整篇的偏移量，所以编辑器选中它们时不需要换算。
- **警告检查器按上游重排成两栏**：`Inspector ▸ Warnings` 原来是一列自己编的提示文字，
  现在是上游那两块 —— **Incompatible Characters**（Line / Character / Converted 三列）
  与 **Inconsistent Line Endings**（Line / Line Ending 两列），各带一句状态行
  （`Found N …` 或 `No issues found.`），两栏之间可以拖动改高，点一行把编辑器选区
  放到那个字符上。「不兼容」的判定照上游：**有损编码再解码回来，不等就是有问题**，
  一段里不止一个字符就对半切开递归 —— 所以 Shift-JIS 里 `\` 会变成 `＼` 这种事也抓得到。
- **保存不再静默写坏文件**：`Document::save` 一直丢掉了编码器的 `had_errors` 标志，
  于是把一份带 emoji 的文档按 Shift-JIS 保存，磁盘上会**静默**出现 `&#65533;`，
  而文件看起来保存成功了。现在会拒绝并报出上游那句话
  （*The document contains characters incompatible with "Japanese (Shift JIS)"*），
  磁盘上什么都不会留下。上游给的是三个选择（*Save Available Text* / *Change Encoding* /
  *Cancel*），SubText 做的是「取消」，并把「改编码」指到状态栏的编码选择器上。
- **补全（`Edit ▸ Complete`，⌥⎋）**：此前是占位提示，现在是上游那套 ——
  在光标处的词上按下，弹出候选列表（文档里以它开头的词 + 语法 `Completion.json` 里
  以它开头的词），↑↓ 选、Enter/Tab 插入、Esc 或左右方向键取消，继续打字会重新过滤。
  词里带括号的（`foo()`）插完光标停在括号中间。Mode 面板的
  *Words in document* / *Words defined in syntax* 两个开关**第一次真的生效**。
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
- **三个快捷键在生成菜单表时被 base64 吃掉了**：storyboard 里凡是**控制字符**做键位
  的地方（Interface Builder 不能把控制字符写成 XML 属性），存的是
  `<string key="keyEquivalent" base64-UTF8="YES">`。提取时没解这层，于是
  **Edit ▸ Delete（⌘⌫）**、**Text ▸ Delete Line（⇧⌘⌫）** 与
  **Edit ▸ Complete（⌥⎋）** 三条键位读成了空串，快捷键表一直少三条。
  `Delete Line` 最刺眼 —— 命令本身早就能用，只是快捷键丢了。生成器现在会解这两个
  控制字符，快捷键表从 72 条变成 **75 条**。
- **`Edit ▸ Delete` 此前只是弹一句提示**：它绑的是「Handled by the text engine」，
  而 AppKit 的文本视图并不绑定 ⌘⌫，所以那条快捷键等于没有。上游的 `delete:` 是
  「删选区，否则删光标前一个字符」，现在照做（单位是扩展字形簇，`👍🏻` 一次删干净）。
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
- **`⌘F` 打开的面板拿不到光标**：焦点是在**文档窗口**上给的
  （`focus_handle(cx).focus(window, cx)`），而 GPUI 的 `Window::focus` 是**按窗口存的**
  —— 文档窗口于是拿着一个它根本不画的句柄，面板的查找框反而没有焦点，键盘输入
  不会进去。改成在面板自己的窗口里给焦点。这条是照抄上游那句
  `initialFirstResponder = textView` 的字面意思才发现的：**「谁是第一响应者」是窗口的属性**。
- **查找字段画了两层边框**：上游只有一层（`strokeBorder(.separator)` 叠在裁圆角的字段上），
  而 SubText 在外层 `div` 的描边之外，`Input` 组件自己又画了一层。现在把组件那层关掉，
  由外层负责，并按 `is_focused(window)` 在 `border` / `ring` 之间切换描边色 ——
  等价于上游的「分隔色描边 + exterior 焦点环」，而且只有一层。
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

**655** 项测试全部通过（v0.1.0 是 124 项）：

| Crate | 数量 |
| --- | --- |
| `subtext-core` | 382 |
| `subtext-app` | 236 |
| `subtext-syntax` | 29 |
| `subtext-theme` | 7 |
| doctest | 1 |
| **合计** | **655** |

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
