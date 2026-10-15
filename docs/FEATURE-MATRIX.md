# SubText 功能对照表

> 配套文档：[评估报告](EVALUATION.md) · [实施计划](PLAN.md) · [架构说明](ARCHITECTURE.md)
>
> 状态图例：**✅ 完整** · **🔁 等价实现** · **⚠️ 平台受限** · **⏳ 未实现**

---

## 0. 总览

| 维度 | CotEditor | SubText | 状态 |
| --- | --- | --- | --- |
| 菜单结构 | `Main.storyboard` 258 项 | `menu.rs` 258 项 | ✅ 逐项一致 |
| 顶层菜单 | 28 个路径 | 28 个路径 | ✅ |
| 命令项 | 168 | 168 | ✅ |
| 快捷键 | 72 条 | 72 条 | ✅ 键位一致 |
| 动作类型 | 155 个 `@objc` selector（storyboard 实测：170 个 `<action>` 元素、去重 155 个） | 176 个类型化动作（`actions.rs`：168 个单元动作 + 8 个带载荷的动作） | 🔁 拆分更细 |
| 语法定义 | 70 个 `.cotsyntax` | 同一批文件 | ✅ 直接复用 |
| 主题 | 14 个 `.cottheme` | 同一批文件 | ✅ 直接复用 |
| 代码行数 | 约 72,128（Swift） | 34,711（Rust，`find crates -name '*.rs'` 去掉生成的 `menu.rs` / `keymap.rs`） | — |

---

## 1. 应用菜单（SubText / CotEditor）

| 功能 | CotEditor 实现 | SubText | 状态 |
| --- | --- | --- | --- |
| 关于面板 | `orderFrontStandardAboutPanel:` | `AboutView`（`dialogs.rs`） | 🔁 |
| 设置窗口 | `SettingsWindowController`（9 面板） | `SettingsWindow`（9 面板，标签 / 顺序 / 分组逐项对齐） | ✅ |
| 服务 | `NSMenu` Services | `MenuItem::os_submenu(_, SystemMenuType::Services)` | ✅ 交系统填 |
| 隐藏 / 隐藏其他 / 全部显示 | `NSApplication` | 交窗口系统处理 | 🔁 |
| 退出 | `terminate:` | `cx.quit()` | ✅ |
| 捐赠窗口 | 独立窗口 | 状态栏提示 | ⏳ |
| 更新日志 | 独立窗口 | 状态栏提示 | ⏳ |

## 2. 文件菜单

| 功能 | SubText 方法 | 状态 |
| --- | --- | --- |
| 新建 | `new_tab` | ✅ |
| 新建标签页 | `new_tab` | ✅ |
| 新建窗口 | — | ⏳ 提示「SubText 以标签页打开文档」 |
| 打开… | `open_dialog` | ✅ |
| 打开最近使用 | 菜单运行时列出最近 10 条（标题用文件名）+ 侧栏 `FileBrowser` | ✅ |
| 清除最近使用 | `clear_recent` | ✅ |
| 关闭 | `close_active` | ✅ |
| 存储 | `save(false)` | ✅ |
| 存储为… | `save(true)` | ✅ |
| 复原到已存储 | `revert` | ✅ |
| 在文件管理器中显示 | `show_in_finder` | ✅ |
| 拷贝路径 | `copy_path` | ✅ |
| 共享 | — | ⏳ |
| 页面设置 / 打印 | — | ⏳ |

**文件 I/O 细节**：编码（含 6 种 BOM）、换行符、文件修改标记、未保存确认，
全部由 `subtext-core::document` + `subtext-core::encoding` 处理。

## 3. 编辑菜单

| 功能 | SubText | 状态 |
| --- | --- | --- |
| 撤销 / 重做 | 编辑器键上下文 | 🔁 由文本引擎处理 |
| 剪切 / 拷贝 / 粘贴 | 编辑器键上下文 | 🔁 |
| 原样粘贴 | 编辑器键上下文 | 🔁 |
| 带样式拷贝 | — | 🔁 提示「由文本引擎处理」 |
| 删除 / 全选 | 编辑器键上下文 | 🔁 |
| 选择单词 | `select_word` | ✅ |
| 选择段落 | `select_line` | ✅ |
| 选择包围符号 | `select_enclosing_symbols` | ✅ |
| 向上/下扩展列选择 | — | ⏳ 需要编辑器的视觉行模型 |
| 按行拆分选区 | `split_selection_by_lines` | ✅ |
| 补全 | — | ⏳ |
| 输入反斜杠 / 日元符号 | `insert_text` | ✅ |
| 显示 Unicode 输入面板 | — | ⏳ |

## 4. 拼写与替换

| 功能 | 状态 |
| --- | --- |
| 显示拼写检查 / 立即检查 | ⚠️ macOS-only |
| 连续拼写检查 | ⚠️ |
| 语法检查 | ⚠️ |
| 自动拼写纠正 | ⚠️ |
| 智能引号 / 破折号 / 文本替换 | ⚠️ |
| 智能插入删除 | ⚠️ |
| 开始朗读 / 停止朗读 | ⚠️ |
| 引号拉直 | ✅ `textops::straighten_quotes`（`[‘’‚‛]`→`'`、`[“”„‟]`→`"`）；空选区时提示「Select something first」 |
| 引号替换 / 破折号替换 / 替换文本 | ⚠️ macOS-only |

> **保留策略**：这些菜单项**全部保留在菜单栏中**（保真度要求菜单一致），
> 触发时在状态栏说明「系统拼写与语音服务仅 macOS 可用」。
>
> **引号拉直是这一组里唯一的例外**：它是 CotEditor 自己的纯变换
> （`EditorTextView+Transformation.swift` 的 `straightenQuotesInSelection:` →
> `String.straighteningQuotes`），与平台无关，已实现（见 `docs/PLAN.md` §8.9h）。
> 另外三条 `replaceQuotesInSelection:` / `replaceDashesInSelection:` /
> `replaceTextInSelection:` 不在 AppKit 公开头文件里，实测「实例响应、类不响应」，
> 即 `NSTextView` 的私有实现（替换面板的系统服务），GPUI 无对应能力。

## 5. 格式菜单

| 功能 | SubText 方法 | 状态 |
| --- | --- | --- |
| 重新着色 | `rehighlight` | ✅ |
| 换行符 LF / CR / CRLF | `set_line_ending` | ✅ 经 Format ▸ Line Endings 菜单，当前项打勾 |
| 换行符 NEL / LS / PS | `set_line_ending` | ✅ **超出多数编辑器**；上游按住 Option 才显示这 3 项，这里恒列（GPUI 菜单没有隐藏项） |
| 编码（转换） | `change_encoding` | ✅ 经 Format ▸ Text Encoding 菜单：列表来自用户的优先级列表（默认 38 条 + 9 个分隔符），按组分隔，当前项打勾（带 BOM 的 UTF-8 勾在 *with BOM* 项上），末尾分隔符 + `Customize Encodings List…`。上游 60 条里 22 条无 codec 如实缺席、8 条为超集近似，逐条见 PLAN §8.9f |
| 编码（重新解释） | `reinterpret_encoding` | 🔁 能力在，但没有入口 —— 上游的菜单里也没有这一项 |
| 编码自动探测 | `Document::open` / `revert` 的 `DetectionOptions` | ✅ 与上游同序：BOM → 声明（`refer_to_encoding_tag`）→ **按用户的优先级列表逐个试** → 默认编码。没有统计猜测（§8.11 删掉了 `chardetng`） |
| 编码列表编辑 | `encoding_list_sheet`（*Edit List…* / `Customize Encodings List…`） | ✅ 增删改序：Add 菜单（分隔符 + 尚未列入的可用编码，按名排序）、Delete（UTF-8 与默认编码拒绝并给出原因）、上移/下移、Restore Defaults；OK 才写回设置，Cancel 丢弃。那份列表同时驱动编码菜单与探测优先级。手势与上游不同（箭头代替拖拽、按钮代替 Delete 键），见 PLAN §8.11 |
| 语法（设置） | `set_syntax` | ✅ 经 Format ▸ Syntax 菜单（None 在前 + 分隔符 + 全部语法，当前项打勾） |
| 标准字体 | `set_font(None)` | ✅ |
| 等宽字体 | `set_font(mono_font())` | ✅ |
| 增大 / 减小字号 | `bump_font(±1.0)` | ✅ |
| 重置字体 | `set_font` + `set_font_size` | ✅ |
| 字体面板 | — | ⏳ 提示走 Format ▸ Font |
| 抗锯齿 | `toggle_antialias` | ✅ |
| 连字 | `toggle_ligatures` | ✅ |
| 主题 | `apply_theme_by_name` | ✅ 经 Format ▸ Theme 菜单，当前项打勾 |
| 制表符宽度 | `cycle_tab_width` | ✅ |
| 自动展开制表符 | `toggle_auto_expand_tabs` | ✅ |
| 显示不可见字符 | `update_editor_options` | ✅ |
| 缩进参考线 | `update_editor_options` | ✅ |
| 页面参考线 | `update_editor_options` | ✅ |
| 自动折行 | `update_editor_options` | ✅ |
| 可编辑 | `update_editor_options` | ✅ |
| 书写方向 LTR / RTL | — | ⏳ |
| 布局方向 水平 / 垂直 | — | ⏳ |

## 6. 显示菜单

| 功能 | SubText 方法 | 状态 |
| --- | --- | --- |
| 标签栏 | `update_window_options` | ✅ |
| 工具栏 | `update_window_options` | ✅ |
| 工具栏自定义 | — | ⏳ |
| 快速操作 | `show_quick_actions` | ✅ 浮动面板 + 子序列匹配菜单路径；空查询不列结果 |
| 快速操作的命令来源 | `NSApp.actionCommands`（遍历实时主菜单） | `menus::commands`（重建菜单栏再摊平） | ✅ 含运行时条目；排除打开面板自身那条 |
| 文件浏览器 | `show_sidebar(FileBrowser)` | ✅ |
| 文件夹查找 | `show_sidebar(FolderFind)` | ✅ |
| 侧边栏 | `update_window_options` | ✅ |
| 检查器 | `update_window_options` | ✅ |
| 文稿检查器 | `show_inspector(Document)` | ✅ |
| 大纲检查器 | `show_inspector(Outline)` | ✅ 可折叠 `Tree` + 子序列过滤 + 光标跟随 |
| 警告检查器 | `show_inspector(Warnings)` | ✅ |
| 导航栏 | `update_window_options` | ✅ |
| 行号 | `update_editor_options` | ✅ |
| 状态栏 | `update_window_options` | ✅ |
| 下一个 / 上一个编辑器 | `next_tab(±)` | ✅ |
| 分屏 / 关闭分屏 / 切换方向 | `open_split_editor` / `close_split_editor` / `toggle_split_orientation` | ✅ |
| 全屏 | `toggle_fullscreen` | ✅ |
| 保持在最前 | `update_window_options` | ✅ |

## 7. 文本菜单（保真度最高的部分）

| 功能 | SubText 方法 | 状态 |
| --- | --- | --- |
| 切换注释 | `toggle_comment` | ✅ |
| 行内注释 | `toggle_comment(inline only)` | ✅ |
| 块注释 | `toggle_comment(block only)` | ✅ |
| 取消注释 | `toggle_comment` | ✅ |
| 向右 / 向左缩进 | `shift_right` / `shift_left` | ✅ |
| 缩进转空格 / 转制表符 | `convert_indentation_to_*` | ✅ |
| 用引号 / 双引号 / 括号 / 花括号 / 方括号包围 | `surround` | ✅ 5 种 |
| 自定义包围 | — | ⏳ |
| 修剪行尾空白 | `trim_trailing_whitespace` | ✅ |
| 上移 / 下移行 | `move_lines` | ✅ |
| 按行升序排序 | `sort_lines` | ✅ |
| 按模式排序 | `pattern_sort_dialog` / `sort_lines_by` | ✅ 三种键 |
| 反转行 | `reverse_lines` | ✅ |
| 随机打乱行 | `shuffle_lines`（确定性 xorshift） | ✅ |
| 删除重复行 | `delete_duplicate_lines` | ✅ |
| 复制行 | `duplicate_lines` | ✅ |
| 删除行 | `delete_lines` | ✅ |
| 合并行 | `join_lines` | ✅ |
| 转大写 / 小写 / 首字母大写 | `transform_case` | ✅ |
| 全角 / 半角 / 全角罗马字 / 半角罗马字 | `transform_width` | ✅ 4 种 |
| URL 编码 / 解码 | `url_encode` / `url_decode` | ✅ |
| Unicode 规范化 | `normalize_unicode` | ✅ 经 Text ▸ Unicode Normalizations 菜单（5 项 + 分隔符 + 2 项）；上游 7 种里 4 种可用，3 个 Apple 形态触发时在状态栏说明原因 |
| 显示选区信息 | `inspect_selection` | ✅ |
| 高级计数 | `inspect_document` | ✅ |
| 编辑颜色代码 | `color_code_dialog` | ✅ 11 种记法 |
| 插入片段（子菜单） | `menus::install` 按语法动态重建 | ✅ 无范围片段在前 + 按语法分节 |
| 代码片段编辑器 | `show_settings_pane(Snippets)` | ✅ 两个页签 + 令牌插入菜单 |
| 文件拖放 | `drop_files` | ✅ 11 个令牌 / 五种图片格式 |

> **文本菜单是保真度最高的部分**：`subtext-core::textops` 里的文本操作全部实现，
> 全是纯函数，全部有单元测试覆盖。菜单里只剩「自定义包围」一项仍是占位提示。

## 8. 查找菜单

| 功能 | SubText 方法 | 状态 |
| --- | --- | --- |
| 查找 | `show_find(false)` | ✅ |
| 替换 | `show_find(true)` | ✅ |
| 查找下一个 / 上一个 | `find_step(±)` | ✅ |
| 查找全部 | `find_all` | ✅ |
| 选择全部查找结果 | `find_all` | ✅ |
| 用所选内容查找 | `use_selection_for_find` | ✅ |
| 用所选内容替换 | `use_selection_for_replace` | ✅ |
| 居中显示选区 | — | 🔁 提示已跳转 |
| 替换并查找下一个 | `replace_and_find_next` | ✅ |
| 全部替换 | `replace_all` | ✅ |
| 多重替换 | `multiple_replace_dialog` | ✅ 规则表 + TSV 导入模型 |
| 高亮全部 / 移除高亮 | `highlight_all` / `clear_highlights` | ✅ |
| 打开大纲菜单 | `show_inspector(Outline)` | ✅ |
| 上一个 / 下一个大纲项 | `select_previous/next_outline_item` | ✅ 落在条目匹配的范围上 |
| 跳转到指定行 | `goto_dialog` | ✅ |

**查找选项**：正则、区分大小写、全字匹配、循环查找、反向查找
（`finder::FindOptions`）。

## 9. 窗口与帮助菜单

（`Script` 菜单排在 `Window` 与 `Help` 之间，内容是运行时的，单列于 §10。）

| 功能 | SubText | 状态 |
| --- | --- | --- |
| 最小化 | `minimize_window` | ✅ |
| 缩放 | `zoom_window` | ✅ |
| 前置全部窗口 | 交窗口系统处理 | 🔁 |
| 显示控制台 | — | ⏳ |
| 帮助 / 打开帮助锚点 | — | ⏳ 指向 `docs/` |
| AppleScript 字典 | — | ⚠️ GPUI 无此能力 |
| 官网 / 报告问题 / 创建报告 / 发行说明 | 状态栏提示 | 🔁 |

---

## 10. Script 菜单

顶层菜单本身在 `menu.json` 里是**空的**（`{"path": "", "title": "Script", ...}`），
和上游一样——它的内容来自用户的脚本文件夹，构建时才填。目录落点：
macOS 为 `~/Library/Application Scripts/com.subtext.SubText`（上游 `ScriptManager` 的路径），
Windows / Linux 为 `config_directory()/Scripts`。

| 功能 | CotEditor 实现 | SubText | 状态 |
| --- | --- | --- | --- |
| 脚本列表 | `ScriptManager.updateMenu()` → `scriptMenuItems(at:)` | `scripts::scan()` → `scripts::items()` | ✅ |
| 目录递归成子菜单 | `ScriptManager` | `Item::Folder` | ✅ |
| `-` 文件变分隔符 | 同上 | `Item::Separator` | ✅ |
| `^\d+\)` 前缀去除（排序用） | 同上 | `scripts::menu_name` | ✅ |
| 跳过 `.` / `_` 前缀文件 | 同上 | `scan` | ✅ |
| 末尾 `Open Scripts Folder` | `[.separator(), "Open Scripts Folder"]` | `scripts::OPEN_SCRIPTS_FOLDER` | ✅ |
| 脚本扩展名快捷键 | `ScriptDescriptor` 解析扩展名 | `Shortcut::parse`（`Clean.@s.sh` → ⌘S） | ✅ |
| 语法同名子菜单优先派键 | `applyShortcuts()` | `scripts::bindings(items, scope)` | ✅ |
| `%%%{CotEditorXInput=…}%%%` | `UnixScript.run()` | `Invocation::scan` | ✅ |
| `%%%{CotEditorXOutput=…}%%%`（七种目标） | 同上 | `OutputType::range` + `apply_script_output` | ✅ |
| AppleScript（`.applescript` / `.scpt` / `.scptd`） | `ScriptingFileType`，经 Apple event | macOS 交给 `osascript`；其他平台不进菜单 | 🔁 |
| shebang 脚本（`.sh` / `.py` 等） | 内核执行 | `Launch::File`；Windows 按扩展名给解释器 | 🔁 |
| 目录变化自动重算菜单 | `NSFilePresenter` | 2 秒轮询（`SCRIPT_POLL`）+ `MenuSignature` | 🔁 |
| Option 点击打开 / Option+Shift 显示 | `NSEvent.modifierFlags` | — | ⚠️ GPUI 菜单项无修饰键状态，点击恒为运行 |
| 菜单项 tooltip | `"Option-click to open script in editor."` | — | ⚠️ GPUI 菜单项无 tooltip |
| 事件脚本（`documentOpened` / `documentSaved`） | `ScriptInfo` + `CotEditorHandlers` | — | ⏳ 无对应钩子 |
| `tell application "CotEditor"` | 自身 scripting dictionary | — | ⚠️ 不暴露 dictionary |

---

## 11. 快捷键对照（72 条）

全部 72 条与 CotEditor 一致。修饰键语义：

| CotEditor（Cocoa） | SubText（GPUI） | 显示 |
| --- | --- | --- |
| `command` | `secondary` | ⌘ |
| `shift` | `shift` | ⇧ |
| `option` | `alt` | ⌥ |
| `control` | `ctrl` | ⌃ |

> `secondary` 在 macOS 上是 Command，在 Windows / Linux 上是 Control ——
> 这正是跨平台编辑器应有的行为。

常用键位抽样：

| 快捷键 | 命令 |
| --- | --- |
| ⌘N / ⇧⌘N | 新建 / 新建标签页 |
| ⌘O / ⌘S / ⇧⌘S | 打开 / 存储 / 存储为 |
| ⌘F / ⌥⌘F | 查找 / 替换 |
| ⌘G / ⇧⌘G | 查找下一个 / 上一个 |
| ⌥⌘E | 全部替换 |
| ⌘/ | 切换注释 |
| ⌘] / ⌘[ | 向右 / 向左缩进 |
| ⌥⌘↑ / ⌥⌘↓ | 上移行 / 下移行 |
| ⌃⌘↑ / ⌃⌘↓ | 选择上一个 / 下一个大纲项 |
| ⌘L | 跳转到指定行 |
| ⇧⌘T | 显示/隐藏工具栏 |
| ⇧⌘L | 显示/隐藏行号 |
| ⌃⌘F | 全屏 |

完整表见 `crates/subtext-app/src/keymap.rs` 的 `SHORTCUTS` 常量，
或在应用的 **设置 ▸ 快捷键** 面板中查看。

---

## 12. 统计

| 状态 | 数量 | 占比 |
| --- | --- | --- |
| ✅ 完整实现 | 约 129 | 64% |
| 🔁 等价实现（委托或改写） | 约 24 | 12% |
| ⚠️ 平台受限（菜单保留） | 约 20 | 10% |
| ⏳ 未实现（菜单保留 + 提示） | 约 28 | 14% |
| **合计** | **201** | **100%** |

**关键点**：**菜单栏 258 项一个不少**。
未实现的功能不是「从菜单里删掉」，而是「保留 + 说明」——
用户看到的菜单结构与 CotEditor 完全相同，只是部分条目在触发时给出诚实的反馈。
