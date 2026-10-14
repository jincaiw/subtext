# SubText UI 审计报告（v0.1.0 → v0.2）

> 依据：[gpui-kit 设计规范] 与 [gpui-kit 编码规范]（技能 `gpui-kit-design-guides`、
> `gpui-kit`）。规范是**要求**，不是参考。
>
> 审计对象：`crates/subtext-app/` 的全部 UI 代码。
> 每条发现给出文件行号、违反的规范条款、以及修复方向。

---

## 0. 先说结论

v0.1.0 的功能面是完整的（菜单 258 项、快捷键 72 条、动作 182 个），
但 UI 层是在**没有读过这两份规范**的情况下写的，因此系统性偏离集中在四类：

| 类别 | 数量 | 严重度 |
| --- | --- | --- |
| 布局用了 `px()` 字面量而非 rem 缩放 | 7 处 | 中 |
| `ElementId` 用列表下标而非领域 ID | 4 处 | **高** |
| 用 `primary()` 按钮冒充「选中」状态 | 5 处 | **高** |
| 滚动归属缺失 / 可点击 div 无焦点 | 6 处 | **高** |

两个**已经正确**、但值得记录的点（避免重复排查）：

- `gpui_kit::open_window` 内部已经 `cx.new(|cx| base::Root::new(view, window, cx))`
  （`crates/kit/src/lib.rs:185`），所以应用不需要自己包 `Root`。
- rem 基准已由组件的 `WindowState` 插件在 `prepare` 钩子里设置：
  `window.set_rem_size(cx.theme().font_size)`（`crates/component/src/root.rs:509`）。
  **正因为 rem 已生效，用 `px()` 写布局才是真偏离** —— 那些值不随基准字号缩放。

---

## 1. 违反「Tokens before values」：布局用了 `px()` 字面量

规范原文：

> Treat every direct `px(...)` and raw color constructor in application UI as a
> review finding. Accept it only for a documented physical/platform boundary,
> measured runtime geometry, raster/data color, or the theme/token definition
> itself.

### 1.1 需要改（7 处）

| 位置 | 现状 | 应改为 |
| --- | --- | --- |
| `workspace.rs:605` | 查找栏输入框 `.w(px(320.))` | rem 档位 |
| `workspace.rs:1169` | 字符检查对话框 `.w(px(360.))` | rem 档位 |
| `workspace.rs:1205` | 文档统计对话框 `.w(px(360.))` | rem 档位 |
| `workspace.rs:1394` | 侧栏 `.w(px(240.))` | rem 档位 |
| `workspace.rs:1486` | 检查器 `.w(px(260.))` | rem 档位 |
| `workspace.rs:1566` | 大纲缩进 `.pl(px(indent))` | 用 rem 档位表达层级 |
| `workspace.rs:1668`, `1670` | 查找/替换输入框 `.w(px(220.))` | rem 档位 |
| `settings_window.rs:107` | 设置导航栏 `.w(px(180.))` | rem 档位 |

### 1.2 属于规范允许的例外，保留并加注释

| 位置 | 理由 |
| --- | --- |
| `workspace.rs:1222`, `1241` | 窗口最小尺寸 —— 平台窗口几何，不是设计尺度 |
| `workspace.rs:1558` | `h(px(1.))` 分隔线 —— 物理像素发丝线 |
| `workspace.rs:1785` | `.text_size(px(self.settings.font_size))` —— 文档字号是**用户数据**，不是 UI 缩放 |

> 这三类正是规范列举的合法例外。修复时要**保留它们并写明理由**，
> 否则下一轮审计会把它们当成遗漏。

---

## 2. 违反「Stable identity」：`ElementId` 用了列表下标

规范原文：

> Never derive identity from a translated label or a mutable list index when
> items can be inserted or reordered.

| 位置 | 现状 | 为什么是错的 |
| --- | --- | --- |
| `workspace.rs:1325` | `format!("tab-{index}")` | 标签可以关闭、将来可以拖拽重排 —— 下标会错位 |
| `workspace.rs:1340` | `format!("tab-close-{index}")` | 同上 |
| `workspace.rs:1419` | `format!("recent-{index}")` | 最近文件列表随打开/清除而变动 |
| `workspace.rs:1565` | `format!("outline-{index}")` | 大纲随编辑内容重算，项会插入/删除 |

**修复方向**：给文档分配稳定的领域 ID（单调递增的 `DocumentId`），
标签用 `("tab", id)`、关闭按钮用 `("tab-close", id)`；
最近文件用路径的哈希或路径本身；大纲项用 `(行号, 标题)` 组合键。

> 规范补充：「A changed ID means a changed UI identity. Treat that reset as
> deliberate.」—— 下标 ID 会让「关掉第一个标签」意外重置所有后续标签的
> 元素状态（悬停、滚动位置、动画）。

---

## 3. 违反「Interaction states / 语义组件」：用 `primary()` 冒充「选中」

规范原文：

> Selection is part of the information model, not only a pointer effect.
> Use the semantic state style of the component for selected, checked,
> pressed, and disabled appearance.

> `primary` is reserved for the explicit default commit in a decision area.

现状是 5 处「多选一」控件用 `Button::primary()` 表示当前项：

| 位置 | 控件 | 问题 |
| --- | --- | --- |
| `workspace.rs:1374-1384` | 侧栏面板切换（文件浏览器 / 文件夹查找） | 应是 segmented 控件 |
| `workspace.rs:1466-1476` | 检查器切换（文稿 / 大纲 / 警告） | 同上 |
| `settings_window.rs:80-89` | 设置导航（5 个面板） | 同上 |
| `settings_window.rs:142-151` | 默认换行符（LF / CRLF / CR） | 同上 |
| `settings_window.rs:174-185` | 默认编码（7 项） | 同上 |

**为什么这不是风格问题**：`primary` 是「决策区的默认提交」。
用它表示「当前选中」，会让界面同时出现多个视觉上的「主按钮」，
用户无法判断哪个是「确认」。而且这些是**单选**语义，
规范要求「A segmented control is one silhouette」——
当前实现是两个各自圆角的独立按钮加间隙，不是一体的分段控件。

**修复方向**：`TabBar::new(id).segmented().selected_index(ix).children(tabs)`。
gpui-kit 的 `TabBar` 已提供 `segmented()` / `pill()` / `underline()` /
`folder()` 四种变体，直接选 `segmented` 即可获得正确的一体轮廓、
键盘导航、焦点转移与无障碍契约。

---

## 4. 违反「语义组件」：可点击 `div` 与手搓标签栏

规范原文（列为明确的失败模式）：

> custom clickable `div`s where a semantic component already supplies focus,
> keyboard, disabled, and accessibility behavior

| 位置 | 现状 | 问题 |
| --- | --- | --- |
| `workspace.rs:1324-1352` | 标签栏是手搓 `h_flex` + `cursor_pointer` + `on_click` | 无键盘导航、无焦点环、无 `aria` 角色 |
| `workspace.rs:1418-1431` | 最近文件项是可点击 `h_flex`，仅 `.hover()` | 无法用键盘到达，无焦点状态 |
| `workspace.rs:1563-1574` | 大纲项同上 | 同上 |

**修复方向**：

- 标签栏 → `TabBar`（已支持 `suffix`，可放关闭按钮）。
- 最近文件 → `List` + `ListDelegate`，或至少 `ListItem`。
- 大纲 → `List`，长期应评估 `Tree`（见 §6）。

---

## 5. 违反「Scroll ownership」：内容被裁且无滚动

规范原文：

> Every scrollable region must have one owner. […] Attach `Scrollable` to the
> element that owns the full panel, editor, or window viewport so its scrollbar
> resolves against the region edge.

| 位置 | 现状 | 后果 |
| --- | --- | --- |
| `workspace.rs:1401` | 侧栏 body `div().flex_1().overflow_hidden()` | 最近文件超过 30 条时**被裁且无法滚动** |
| `workspace.rs:1493` | 检查器 body 同上 | 大纲项多时被裁 |

对照：`settings_window.rs:118-125` 是**正确**的 ——
`.id("settings-pane").flex_1().min_w_0().overflow_y_scroll()`，滚动归属清晰。

**修复方向**：按同样的写法给侧栏与检查器 body 加 `.id(...)` + `overflow_y_scroll()`，
并给可收缩的子项加 `min_h_0()`。

---

## 6. 违反「Designing data-heavy interfaces」：层级用缩进模拟

规范原文：

> A tree for real hierarchy.

`workspace.rs:1561-1566` 用 `item.level as f32 * 10.0` 算出像素缩进，
把大纲的树形结构压平成一张缩进列表。

这不只是观感问题：缩进列表没有折叠/展开、没有父节点可达性、
没有键盘的左右键语义。

**修复方向**：`outline.rs` 已经产出 `level: usize`，可以直接喂给
`tree::{Tree, TreeState, TreeItem}`。这是 v0.2 里收益最明显的一处。

---

## 7. 无障碍：图标按钮没有名字

规范原文：

> Icon-only buttons always need an accessible name.

| 位置 | 按钮 | 现状 |
| --- | --- | --- |
| `workspace.rs:1265` | 新建 | 只有 `IconName::Plus` |
| `workspace.rs:1272` | 打开 | 只有 `IconName::FolderOpen` |
| `workspace.rs:1279` | 存储 | 只有 `IconName::HardDrive` |
| `workspace.rs:1294` | 侧栏开关 | 只有 `IconName::PanelLeft` |
| `workspace.rs:1305` | 检查器开关 | 只有 `IconName::PanelRight` |
| `workspace.rs:1712` | 关闭查找栏 | 只有 `IconName::Close` |

`Button` 同时提供 `.accessibility_label(...)` 与 `.tooltip(...)`
（`crates/component/src/button/button.rs:393, 405`），两个都要加：
前者给屏幕阅读器，后者给指针用户。

---

## 8. 已确认正确的部分（不必改）

| 项 | 证据 |
| --- | --- |
| `Root` 已就位 | `gpui_kit::open_window` 内部包 `base::Root`（`kit/src/lib.rs:185`） |
| rem 缩放已生效 | 组件 `WindowState::prepare` 设 `set_rem_size(theme.font_size)`（`component/src/root.rs:509`） |
| 先绑键、后设菜单 | `lib.rs:100-101` 顺序正确（规范明确要求） |
| 主题色走语义 token | 全代码无 `rgb(`/`hsla(` 字面量 |
| 设置面板滚动归属正确 | `settings_window.rs:118-125` |
| 单一命令只建模一次 | `actions.rs` → `bridge.rs` → `dispatch.rs` 单一分发 |

---

## 9. 【严重】设置窗口与上游不一致

这一条比上面所有问题都严重，且不是规范问题而是**保真度问题**。

v0.1.0 的设置窗口是**照猜测写的，没有读上游源码**。核对
`.refs/CotEditor/CotEditor/Sources/Settings Window/SettingsPane.swift` 后发现：

### 9.1 面板数量与名称都不对

上游 `SettingsPane` 枚举有 **9 个**面板：

| 上游 | 标签 | SF Symbol | SubText v0.1.0 |
| --- | --- | --- | --- |
| `general` | General | `gearshape` | ⚠️ 存在，但**内容是错的** |
| `appearance` | Appearance | `eyeglasses` | ✓ |
| `window` | Window | `uiwindow.split.2x1` | ✗ **缺失** |
| `edit` | Edit | `square.and.pencil` | ⚠️ 被误命名为 "Editor" |
| `mode` | Mode | `switch.2` | ✗ **缺失** |
| `format` | Format | `text.document` | ⚠️ 存在，但内容被搬到了 General |
| `snippets` | Snippets | `text.viewfinder` | ✗ **缺失** |
| `shortcuts` | Shortcuts | `keyboard` | ⚠️ 被误命名为 "Key Bindings" |
| `integration` | Integration | `apple.terminal.fill` | ✗ **缺失** |

上游面板宽度固定 `580`。

### 9.2 内容错位

SubText 的 "General" 面板放的是**换行符 / 编码 / 语法 / 最近文档**，
而这三项在 CotEditor 里属于 **Format** 面板。

CotEditor 的 General 面板真实内容是
（`Panes/GeneralSettingsView.swift`）：

1. **On startup** — 「Reopen windows from last session」开关 +
   「When nothing else is open」下拉（Create New Document / Show Open Dialog / Do Nothing）
2. **Document save** — 「Enable Auto Save with Versions」开关 + 说明文字
3. **When document is changed by another application** — 单选组
   （Keep CotEditor's version / Ask how to resolve / Update to modified version）
4. **Settings import & export** — Export… / Import… 两个按钮
5. **Dialog warnings** — Manage Warnings… 按钮
6. **Software update** — 「Check for updates automatically」+「Update to prereleases」

### 9.3 根因

写 v0.1.0 时对设置面板是**推断**而不是**核对**。
菜单表和快捷键表因为是机器提取的，所以准确；
设置窗口因为是手写的，所以不准。

**这正是 [评估报告 §1.3](EVALUATION.md) 那条结论的反例**：
「上游的数据全部可复用，行为需要重写」——
设置面板的结构也是「数据」，同样应该先读、再写。

### 9.4 修复方向

1. `SettingsPane` 扩到 9 个变体，标签与图标逐一对齐；
2. `Settings` 模型按上游 `@AppStorage` 键扩展；
3. 各面板按上游 `Grid(alignment: .leadingFirstTextBaseline)` 的
   标签 / 内容两列结构重写；
4. 应用当前无法兑现的开关，**禁用并说明原因**，而不是假装生效。

详见 [PLAN.md](PLAN.md) 的 v0.2 阶段 7.8。

---

## 10. 修复优先级

| 优先级 | 项 | 理由 | 状态 |
| --- | --- | --- | --- |
| **P0** | §9 设置窗口 | 保真度硬伤：9 个面板只做了 5 个，内容错位 | ✅ 已修复 |
| P0 | §2 `ElementId`、§5 滚动、§7 无障碍 | 可导致状态错乱 / 内容不可达 / 屏幕阅读器不可用 | ✅ 已修复 |
| P1 | §3 选中语义、§4 语义组件 | 用户可感知的一致性与键盘可用性 | ✅ 已修复 |
| P2 | §1 `px()` 字面量 | 缩放一致性；改动机械但面广 | ✅ 已修复 |
| P3 | §6 大纲改树 | 体验提升，工作量最大 | ⏳ 待办 |

修复计划见 [PLAN.md](PLAN.md) 的 v0.2 阶段。

---

## 11. 修复记录（v0.2）

### 11.1 一次错误的推断：`gpui_kit::relative`

阶段 7 的第一轮把 11 处 `px()` 换成了 `gpui_kit::relative(..)`，
以为那是「生成 rem 长度」的辅助函数。**它不存在。**

gpui 里表达「随基准字号缩放的长度」的函数是 `rems(f32) -> Rems`，
`impl From<Rems> for Length` 让它可以直接喂给 `.w()` / `.h()` / `.min_w()`。
`gpui_kit::relative` 是本轮凭空写出来的名字，编译即失败。

教训与 §9.3 同一条：**名字要查，不要猜。**
现已全部改为 `gpui_kit::rems(..)`。

例外是 `Dialog::w`——它收 `impl Into<Pixels>` 而不是 `Length`，
所以那里改为 `window.rem_size() * 20.`，手动乘一次界面缩放。

还有一处 `.h(gpui_kit::relative(1.))`（编辑器容器）本意是「占满高度」，
被 `rems(1.)`（= 1 个基准字号的字高）与 `Fraction(1.0)`（= 100%）混淆了。
现改为 `h_full()`。

### 11.2 设置窗口（§9）

`SettingsPane` 由 5 个变体扩到上游的 9 个，顺序与标签逐一对齐：

| 变体 | 标签 | 上游 SF Symbol | 实际图标 |
| --- | --- | --- | --- |
| `General` | General | `gearshape` | `Settings` |
| `Appearance` | Appearance | `eyeglasses` | `Eye` |
| `Window` | Window | `uiwindow.split.2x1` | `Columns2` |
| `Edit` | Edit | `square.and.pencil` | `SquarePen` |
| `Mode` | Mode | `switch.2` | `ToggleLeft` |
| `Format` | Format | `text.document` | `FileText` |
| `Snippets` | Snippets | `text.viewfinder` | `ScanText` |
| `Shortcuts` | Shortcuts | `keyboard` | `Keyboard` |
| `Integration` | Integration | `apple.terminal.fill` | `Terminal` |

`SettingsPane::symbol()` 保留上游的 SF Symbol 名，`icon_for()` 负责映射到
图标集里实际存在的那一个——这样两份导航栏仍然可以逐项对照。

`Settings` 模型按上游 `@AppStorage` 键扩展为六个分组
（`general` / `appearance` / `editor` / `edit` / `mode` / `window`），
字段名沿用上游键名的小写下划线形式，便于回溯。

各面板按上游 `Grid(alignment: .leadingFirstTextBaseline)` 的
「右对齐标签 + 左对齐控件」两列结构重写（`field()` 辅助函数）。

无法兑现的开关一律 `.disabled(true)` 并附说明文字，包括：
Export… / Import… / Manage Warnings… / Edit List…（编码优先级）/
Learn More…（命令行工具）/ Restore Defaults，以及 Mode 面板中
依赖 AppKit 文本引擎的 Smart quotes / Smart dashes / Text replacement /
Add period with double-space / 拼写检查三项。

`settings.rs` 里三个描述同一个决定（缩进用空格还是 Tab、宽度多少）的字段
现在只经由 `Settings::set_indentation()` 写入，杜绝三者漂移。

### 11.3 其余各节

| 节 | 修法 |
| --- | --- |
| §1 | 全部 `px()` → `rems()`；三处合法例外（窗口最小尺寸、发丝分隔线、文档自身字号）保留并加注释 |
| §2 | `DocumentId(u64)` 领域身份；`ElementId::NamedInteger` 用于 tab 关闭键、大纲行、选择列表行 |
| §3 | 5 处 `primary()` 全部改为 `selected()` |
| §4 | 标签栏 / 侧栏 / 检查器 / Snippets 页签改用 `TabBar` + `Tab`；tab 关闭键由 `div` 改为 `Button` |
| §5 | 侧栏 body、检查器 body 各自持有 `overflow_y_scroll()` |
| §7 | 6 个图标按钮补 `accessibility_label` + `tooltip` |

§6（大纲改 `Tree`）留待下一阶段：它需要先把 `OutlineItem` 的层级关系
从「行号 + level」变成真正的树，属于结构改动而非样式改动。

