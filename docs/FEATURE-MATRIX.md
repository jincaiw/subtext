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
| 动作类型 | 153 个 `@objc` selector | 182 个类型化动作 | 🔁 拆分更细 |
| 语法定义 | 70 个 `.cotsyntax` | 同一批文件 | ✅ 直接复用 |
| 主题 | 14 个 `.cottheme` | 同一批文件 | ✅ 直接复用 |
| 代码行数 | 约 72,128（Swift） | 16,704（Rust，不含 `tools/` 生成器） | — |

---

## 1. 应用菜单（SubText / CotEditor）

| 功能 | CotEditor 实现 | SubText | 状态 |
| --- | --- | --- | --- |
| 关于面板 | `orderFrontStandardAboutPanel:` | `AboutView`（`dialogs.rs`） | 🔁 |
| 设置窗口 | `SettingsWindowController`（9 面板） | `SettingsWindow`（9 面板，标签 / 顺序 / 分组逐项对齐） | ✅ |
| 服务 | `NSMenu` Services | 保留菜单项（置灰） | ⚠️ |
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
| 打开最近使用 | `show_sidebar(FileBrowser)` | 🔁 |
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
| 引号拉直 / 引号替换 / 破折号替换 | ⏳ |
| 替换文本 | ⏳ |

> **保留策略**：这些菜单项**全部保留在菜单栏中**（保真度要求菜单一致），
> 触发时在状态栏说明「系统拼写与语音服务仅 macOS 可用」。

## 5. 格式菜单

| 功能 | SubText 方法 | 状态 |
| --- | --- | --- |
| 重新着色 | `rehighlight` | ✅ |
| 换行符 LF / CRLF / CR | `set_line_ending` | ✅ |
| 换行符 NEL / LS / PS | `set_line_ending` | ✅ **超出多数编辑器** |
| 编码（设置 / 重新解释） | `change_encoding` / `reinterpret_encoding` | ✅ 经 Format ▸ Encoding 菜单 |
| 语法（设置） | `set_syntax` | ✅ 经 Format ▸ Syntax 菜单 |
| 标准字体 | `set_font(None)` | ✅ |
| 等宽字体 | `set_font(mono_font())` | ✅ |
| 增大 / 减小字号 | `bump_font(±1.0)` | ✅ |
| 重置字体 | `set_font` + `set_font_size` | ✅ |
| 字体面板 | — | ⏳ 提示走 Format ▸ Font |
| 抗锯齿 | `toggle_antialias` | ✅ |
| 连字 | `toggle_ligatures` | ✅ |
| 主题 | `apply_theme_by_name` | ✅ 经 Format ▸ Theme 菜单 |
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
| 快速操作 | — | ⏳ |
| 文件浏览器 | `show_sidebar(FileBrowser)` | ✅ |
| 文件夹查找 | `show_sidebar(FolderFind)` | ✅ |
| 侧边栏 | `update_window_options` | ✅ |
| 检查器 | `update_window_options` | ✅ |
| 文稿检查器 | `show_inspector(Document)` | ✅ |
| 大纲检查器 | `show_inspector(Outline)` | ✅ |
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
| NFC / NFD / NFKC / NFKD 规范化 | `normalize` | ✅ 4 种 |
| 显示选区信息 | `inspect_selection` | ✅ |
| 高级计数 | `inspect_document` | ✅ |
| 编辑颜色代码 | `color_code_dialog` | ✅ 11 种记法 |
| 代码片段编辑器 | — | ⏳ |

> **文本菜单是保真度最高的部分**：`subtext-core::textops` 里的文本操作全部实现，
> 全是纯函数，全部有单元测试覆盖。菜单里只剩「自定义包围」与「代码片段编辑器」
> 两项仍是占位提示。

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
| 上一个 / 下一个大纲项 | — | ⏳ |
| 跳转到指定行 | `goto_dialog` | ✅ |

**查找选项**：正则、区分大小写、全字匹配、循环查找、反向查找
（`finder::FindOptions`）。

## 9. 窗口与帮助菜单

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

## 10. 快捷键对照（72 条）

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

## 11. 统计

| 状态 | 数量 | 占比 |
| --- | --- | --- |
| ✅ 完整实现 | 约 111 | 61% |
| 🔁 等价实现（委托或改写） | 约 22 | 12% |
| ⚠️ 平台受限（菜单保留） | 约 16 | 9% |
| ⏳ 未实现（菜单保留 + 提示） | 约 33 | 18% |
| **合计** | **182** | **100%** |

**关键点**：**菜单栏 258 项一个不少**。
未实现的功能不是「从菜单里删掉」，而是「保留 + 说明」——
用户看到的菜单结构与 CotEditor 完全相同，只是部分条目在触发时给出诚实的反馈。
