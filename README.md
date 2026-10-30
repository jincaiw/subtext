# SubText

一款跨平台纯文本编辑器，基于 [GPUI Kit](https://github.com/longbridge/gpui-kit) 构建，
是 [CotEditor](https://github.com/coteditor/CotEditor) 的忠实重实现。

支持 **macOS / Windows / Linux**，菜单、快捷键、语法定义、主题与 CotEditor 保持一致。

[![CI](https://github.com/jincaiw/subtext/actions/workflows/ci.yml/badge.svg)](https://github.com/jincaiw/subtext/actions/workflows/ci.yml)

---

## 它是什么

CotEditor 是 macOS 上最优秀的纯文本编辑器之一，但它是 Cocoa 专属的：
`NSTextView`、`NSDocument`、storyboard 里的 `NSMenu`、`@objc` selector 响应链。

SubText 保留**用户能看到和能按到的一切** —— 菜单、快捷键、`.cotsyntax` 语法规则、
`.cottheme` 主题、检查器面板 —— 只把底下的框架换掉。

| CotEditor | SubText |
| --- | --- |
| AppKit `NSTextView` | `gpui-base` 的编辑器状态机 |
| `Main.storyboard` 的 `NSMenu` | 从 `resources/menu.json` 生成的 `menu.rs` |
| `NSResponder` 上的 `@objc` selector | `actions.rs`，一个命令一个类型 |
| storyboard 的 `keyEquivalent` | 同一张表生成的 `keymap.rs` |
| `.cotsyntax` 正则 + tree-sitter | `subtext-syntax`，**同一批 bundle** |
| `.cottheme` | `subtext-theme`，**同一批文件** |

---

## 保真度

| 维度 | 状态 |
| --- | --- |
| 菜单项 | 258 / 258 一致（28 个顶层菜单） |
| 快捷键 | 75 / 75 一致 |
| 语法定义 | 70 个 `.cotsyntax` 全部可解析 |
| 主题 | 14 个 `.cottheme` 全部可解析 |
| 文本操作 | 38 个全部实现（注释、缩进、排序、大小写、全半角、URL、Unicode 规范化…） |
| 编码支持 | UTF-8 / UTF-16 LE,BE / UTF-32 LE,BE + 6 种 BOM；自动探测与上游同序（BOM → 声明 → 用户的优先级列表 → 默认编码），**优先级列表可编辑**，默认编码含字节序标记 |
| 换行符 | LF / CRLF / CR / NEL / LS / PS 六种 |
| 检查器 | 文稿信息（上游的四段：File / Text Settings / Count / Character）/ 大纲（可折叠 `Tree` + 子序列过滤）/ 警告 |
| 侧栏 | 文件浏览器 / 文件夹查找 |
| 面板 | 分屏编辑器、多重替换、按模式排序、编辑颜色代码、快速操作（⌘K）、代码片段、Inspect Character（锚定选区的 popover）、高级计数（选项 sheet + 正文上的胶囊）、查找与替换（独立的浮动窗口） |
| 窗口框架 | 侧栏 / 内容 / 检查器三栏（分隔线可拖动、宽度记住），状态栏属于内容列，每个分屏带一条导航栏 |

未实现的功能**不会从菜单里消失** —— 它们保留在菜单栏中，触发时在状态栏说明原因。
一个吞掉按键的编辑器，比一个承认自己缺什么的编辑器更糟。

完整清单见 [docs/FEATURE-MATRIX.md](docs/FEATURE-MATRIX.md)，
逐条偏差见 [docs/PLAN.md](docs/PLAN.md) 的「已知缺口」表。

---

## 构建

```bash
cargo build            # debug
cargo build --release  # release
cargo test --workspace # 738 项测试
```

要求 Rust 1.99.1（edition 2024；`rust-toolchain.toml` 已钉住，`rustup` 会自己装）。
首次构建会编译 GPUI 全家桶，需要几分钟。

### 运行

```bash
cargo run                    # 空文档
cargo run -- path/to/file    # 打开文件

# 面板冒烟（debug 版）：依次打开 22 个面板/页签后退出，任何一处 panic 都会让退出码非 0
SUBTEXT_PANEL_SWEEP=1 cargo run -- path/to/file
```

截图走查（改界面之后该做的事，冒烟只证明「打开不崩」）：

```bash
tools/peek-panel.sh 14 Settings /tmp/shortcuts.png   # 真窗口
tools/peek-overlay.sh 0 /tmp/surround.png            # 对话框：画在文档窗口里
```

两条命令只是「截哪个窗口」不同：对话框画在文档窗口内部，`screencapture -l` 一样能截到
（§22.24 实测；更早的脚本改截全屏，是因为当时误判 `-l` 截不到，而全屏一旦有别的窗口浮在上面就废了）。

**浮动面板**（Quick Actions，第 21 步）还要多一步：它是 `NSPanel`，**应用不是前台时不上屏**，
截出来是空白。先用 `open <应用>` 让它到前台，再截图；`winlist` 里看到 `onscreen=false` 就是没上屏。改 UI 前先读
[`docs/UI-CHECKLIST.md`](docs/UI-CHECKLIST.md)：那份清单里每一条都是本项目实际踩过的坑。

### 打包（macOS）

```bash
tools/package-macos.sh              # release 构建 + 打包
tools/package-macos.sh --no-build   # 只打包已有的二进制
```

产出 `dist/SubText.app` 与 `dist/SubText-<版本>-<架构>.zip`。
图标由 `tools/make_icon.py` 从几何图形绘制（只用标准库，仓库里不存二进制资源）。

打包出的 `.app` 是**自包含**的：它从 `Contents/Resources/` 解析自己的语法与主题，
不依赖源码树。

---

## 项目结构

```
subtext/
├── crates/
│   ├── subtext-core/     # 编码、BOM、换行符、文档模型、文本操作、查找
│   ├── subtext-syntax/   # .cotsyntax 解析 + 高亮规则引擎
│   ├── subtext-theme/    # .cottheme 解析 + 主题管理
│   └── subtext-app/      # GPUI 应用（窗口、菜单、快捷键、视图）
├── resources/
│   ├── menu.json         # 258 项菜单表（从 CotEditor storyboard 提取）
│   ├── syntaxes/         # 70 个 .cotsyntax
│   ├── themes/           # 14 个 .cottheme
│   └── queries/          # 22 个 .scm 查询
├── tools/
│   ├── generate_menu.py  # 从 menu.json 生成 menu.rs 与 keymap.rs
│   ├── make_icon.py      # 绘制应用图标，产出 .icns
│   └── package-macos.sh  # 打包 SubText.app 与 zip
└── docs/
    ├── EVALUATION.md     # 可行性评估报告
    ├── PLAN.md           # 实施计划
    ├── ARCHITECTURE.md   # 架构说明
    └── FEATURE-MATRIX.md # 功能对照表
```

---

## 文档

- [可行性评估报告](docs/EVALUATION.md) —— 上游盘点、框架能力、风险清单
- [实施计划](docs/PLAN.md) —— 阶段划分、验收标准、非目标
- [架构说明](docs/ARCHITECTURE.md) —— 分层、命令生命周期、关键设计决策
- [功能对照表](docs/FEATURE-MATRIX.md) —— 逐项对照 CotEditor

---

## 许可

Apache-2.0。语法定义与主题文件来自 CotEditor，同样遵循 Apache-2.0。
