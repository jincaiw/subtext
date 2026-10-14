# 更新日志

本文件记录 SubText 的重要变更。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

---

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
