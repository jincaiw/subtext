# GPUI Kit 0.8 升级评估

> 评估对象：`gpui-kit` 0.7.3 → 0.8.1（2026-10-26 发布）。
> 评估方式：把本仓复制到 `/tmp` 换版本号真编译，本仓未被改动。
> 复现命令见 §6。

---

## 1. 结论

1. **该升，且一步到 0.8.1。** 0.8 线的收益集中在编辑器本体 —— tab stop 对齐、
   光标可见性、滚动保真 —— 正是这个项目的卖点；而成本已经被量到具体位置：
   **4 个生产函数 + 3 个测试函数 + 生成器一处**，其余 3 万行 UI 代码零改动。
2. **工具链必须一起动。** 0.8.1 的 `gpui-pre` 0.3.10 用了 Rust 1.96.1 里仍是
   unstable 的 `Option::map_or_default`，本机 stable 连依赖都编译不过；1.99.1 通过。
3. **`secondary-` 是本轮最危险的一条**：69 条静态绑定（75 条里的绝大多数）在
   0.3.10 里**编译期完全静默、启动瞬间 panic**。这与 `docs/EVALUATION.md` §4.1
   记录过的是同一种失败模式（当时是 `control-`），换了个修饰键名而已。
4. 如果工具链暂时不能动，`=0.8.0` 是**零改动**的（实测 `cargo check` 与
   `cargo test` 全过），代价是丢掉 0.8.1 的 tab stop 与光标修复 —— 而那两条
   恰好是本仓最想要的两条。

---

## 2. 版本与实测

| `gpui-kit` | 发布 | 绑定的 `gpui-pre` | 1.96.1（本机 stable） | 1.99.1 |
| --- | --- | --- | --- | --- |
| **0.7.3**（当前） | 10-12 | 0.3.9 | ✅ 现状 | — |
| `=0.8.0` | 10-19 | `=0.3.9` | ✅ **零改动**：`cargo check --all-targets` 零错误，`cargo test` 688 项全过（在 v0.3.7 的树上测；v0.3.45 上未复测） | — |
| **0.8.1** | 10-26 | `=0.3.10` | ❌ 依赖自身编译失败 | ⚠️ 本仓 7 处错误 |

两条容易踩的细节：

- `version = "0.8.0"` 是 caret，会解析到 0.8.1。要真的停在 0.8.0 必须写 `=0.8.0`。
  第一次实测就是因为这个才误把 0.8.0 跑成了 0.8.1。
- 0.8.1 的 3 个依赖错误全在 `gpui-pre-macos-0.3.10/src/window.rs:854/915/1021`
  （`map_or_default`，E0658）。**这是上游代码，本仓没有绕法**，`RUSTC_BOOTSTRAP`
  之类的偏方不值得考虑 —— 工具链本来也该更新。

---

## 3. 必要性：0.8 给了什么

只列与本仓有关的。Shell runtime、WebView、syntect/tree-sitter 高亮、Diff 组件
这些 0.8 的「大件」对本仓都是无关项（高亮走 `.cotsyntax`，没有 WebView 需求）。

### 3.1 0.8.1 —— 直接命中编辑器

| 变更 | PR | 对本仓的意义 |
| --- | --- | --- |
| tab 按 tab stop 测量 / 换行 / 绘制，横向滚动范围按排好的文本宽度 | #3622 #3604 | **最重要的一条。** 纯文本编辑器 + 硬 tab + 软折行是核心场景；`PLAN.md` 里 tab 宽度相关的保真度直接受益 |
| 单行视口的光标裁切、reflow 与高度变化时的光标可见性、垂直滚动 clamp | #3583 #3477 #3527 | 光标贴边被切、滚动到底回弹不准，都是编辑器里最容易被用户看见的毛病 |
| `set_scrollbar_renderer` / `EditorScrollbarContext` | #3644 | CotEditor 的 scroller 可以自己画，不必再迁就组件库默认样式 |
| 获得焦点的输入控件请求外层滚动容器把它露出来 | #3625 | 检查器、设置页里 Tab 到看不见的控件时的老问题 |
| 选择快照每帧只发布一次 | #3686 | 大文档选择时不再无谓地 invalidate 别的 view |
| `Dialog::centered`、DataTable 的居中/右对齐列、`cell_label` | #3635 #3617 | 检查器里的表格、查找结果表 |

### 3.2 0.8.0 —— 滚动与组件

| 变更 | PR | 对本仓的意义 |
| --- | --- | --- |
| macOS 默认 bounce（NSScrollView 手感） | #3500 | 「忠实重实现 CotEditor」加分项，但也是行为变更（§4.4） |
| 文本区能滚时滚轮留在文本里，到末端不外溢 | #3484 | 与 CotEditor 一致，是本仓想要的 |
| DataTable：双击列分隔线 `fit_col`、行模式下 Home/End 选首末行 | #3489 #3471 | 对 `PLAN.md` 里「警告表在窄检查器里横向滚动」有帮助，但不是那一条的答案 —— 列宽可调不等于列可压缩 |
| Tree 在 click 而非 mouse-down 时选中 | #3567 | 大纲树的选中时机与 AppKit 对齐 |
| Tooltip 换触发目标、按下后保持消失、移除触发器即消失 | #3501 #3492 #3528 | 本仓 tooltip 用得很密 |
| 外部编辑器渲染边界 `editor_host` | #3580 | 长期价值：把编辑器渲染与文档模型彻底解耦，本仓已经是这个形状 |

**不构成升级理由的**：0.7.3 → 0.8.1 没有任何一条修复了本仓在 `PLAN.md` 里记账的
具体缺口（编码列表手势、捕获组强调色、大纲不居中、补全列表不虚拟化……）。升级
换的是**手感与正确性**，不是**功能清单**。

---

## 4. 成本：实测到的五件事

### 4.1 工具链（必须先做）

0.8.1 需要 `Option::map_or_default`，1.96.1 报 E0658，1.99.1 通过。本仓没有
`rust-toolchain.toml`，CI 用 `dtolnay/rust-toolchain@stable` —— 也就是说
**能不能编译取决于跑 CI 那天 stable 是多少**。这次升级顺手把它固定下来。

### 4.2 键位是唯一的编译断点

`cargo +1.99.1 check --workspace --all-targets` 在 0.8.1 上只有 7 个错误，全部
集中在 keystroke：

| 位置 | 现状 | 错误 |
| --- | --- | --- |
| `crates/subtext-app/src/snippets.rs:36` | `KeyBinding::new(&keystroke, …)`，键位来自设置文件 | E0597 |
| `crates/subtext-app/src/snippets.rs:53` | `free(keymap, keystroke: &str)` 里的 `Keystroke::parse` | E0521 |
| `crates/subtext-app/src/scripts.rs:102` | `KeyBinding::new(&keystroke, …)`，键位来自脚本文件 | E0597 |
| `crates/subtext-app/src/snippet_pane.rs:218` | `Keystroke::parse(&shortcut.keystroke())` | E0716 |
| `snippets.rs:158/164`、`snippet_pane.rs:884`（测试） | 同上 | E0597 / E0505 / E0521 |

原因是 0.3.10 把这两个函数收紧了：

```rust
// gpui-pre-0.3.10/src/platform/keystroke.rs:120
pub fn parse(source: &'static str) -> Result<Self, InvalidKeystrokeError>;
// gpui-pre-0.3.10/src/platform/keystroke.rs:125
pub fn parse_with_os(source: &str, os: OperatingSystem) -> Result<Self, InvalidKeystrokeError>;

// gpui-pre-0.3.10/src/keymap/binding.rs:35
pub fn new<A: Action>(keystrokes: &'static str, action: A, context: Option<&str>) -> Self;
// binding.rs:51
pub fn load(keystrokes: &str, action: Box<dyn Action>, context_predicate: Option<Rc<…>>,
            use_key_equivalents: bool, action_input: Option<SharedString>, cx: &App) -> Result<Self, …>;
```

动态键位（设置文件里的 snippet、脚本文件里的 script）没有 `&'static str` 可用，
只能走 `KeyBinding::load(…, cx)`，因此 `snippets::bindings` / `scripts::bindings`
要多收一个 `cx: &App`。`menus::install` 里那段借用结构不用改：`cx.key_bindings()`
返回的是 `Rc<RefCell<Keymap>>` 的克隆，`Ref` 活着的时候对 `cx` 再做共享借用是合法的，
块结束后才 `cx.bind_keys(added)`。

### 4.3 `secondary-` 会在启动时 panic，而编译期一声不响

这是本轮**唯一会漏到线上**的坑，单独列出来。

`KeyBinding::new` 内部用的是 `Keystroke::parse`（`os = None`），而 `parse_internal`
遇到 `secondary` 修饰键时要求 `os` 存在，否则返回 `Err`，`new` 里 `.unwrap()`
（`binding.rs:48`）——**panic**。

本仓 75 条默认绑定里有 69 条写成 `secondary-`（`keymap.rs`，由
`tools/generate_menu.py:131` 把 Cocoa 的 `command` 映射成 `secondary`；剩下 6 条是
`ctrl-` / `alt-` 的）。也就是说升到 0.8.1 后：编译通过、`every_binding_parses`
在 `default_bindings()` 里就炸、应用启动即死，栈回溯指向框架而不是业务代码。

`docs/EVALUATION.md` §4.1 记录过一模一样的事（当时是 `control-` 不是 `ctrl-`），
结论也没变：**这条得在生成器里一次性解决，而不是靠测试兜。**

### 4.4 行为变更（不改代码，但必须验）

| 变更 | 本仓受影响的地方 |
| --- | --- |
| macOS 上 `ScrollBounce` 默认开启，组件也默认 bounce | 12 处 `overflow_y_scroll()`：编辑器、侧栏、大纲树、警告表、查找结果、设置页、多重替换 sheet |
| 嵌套滚动不再串联，最内层吃掉整个手势（到末端也不外溢） | `workspace.rs:6580` 警告表嵌在检查器里；`find_panel.rs` 的两层；设置页的列表 |
| 文本区到末端后滚轮不再交给外层 | 主编辑器、检查器里的文本 |
| DataTable 行模式下 Home/End 选首末行；`set_selected_row` 不再 `stop_propagation` | 查找结果表、警告表 |
| `TabVariant::Folder`、`SelectionFormat::SourceAcrossStyles` 新增（非 `#[non_exhaustive]`） | 实测 0 处穷举 match，不受影响 |
| `Easing::sample` 允许 overshoot；`ResizeHandle::on_drag` 参数 `Rc<T>` → `&T` | 实测 0 处使用，不受影响 |

前三条是「手感」层面的，只有跑起来才知道接不接受；第 4 条要确认本仓有没有依赖
传播行为的处理函数。

### 4.5 `test-support` 仍要 `cocoa` 钉版，但 0.8.1 上可用

本仓的 UI 测试靠 `cocoa = "=0.26.0"` 这条 dev-dependency 把 `gpui-fast-macos`
钉死的版本让开（见 `crates/subtext-app/Cargo.toml` 的注释与 CHANGELOG 里
「UI 集成测试可用了」那一条）。0.8.1 把 `gpui-fast` 抬到 0.1.12，这条钉版还要不要、
还能不能压住，只能实测 —— **实测能**：`cargo test -p subtext-app --features ui-tests`
在 0.8.1 上正常解析并编译。升级不会动 UI 测试。

（第一版评估里这一条写成了「仍然不可用」，那是本地 crates.io 索引过期导致的
误判：索引里当时最高只到 `gpui-fast` 0.1.8。刷新索引后 0.1.12 是有的。）

---

## 5. 升级方案

拆成三个可独立回滚的提交，顺序不能变。

### S1 工具链 + 键位（唯一有代码风险的一步）

1. 新增 `rust-toolchain.toml`：`channel = "1.99.1"`，`components = ["rustfmt", "clippy"]`。
   CI 的 `dtolnay/rust-toolchain@stable` 会自动读这个文件，不用改 workflow。
2. `tools/generate_menu.py`：`binding_string()` 不再产出 `secondary-`，改成按平台
   产出 `cmd-` / `ctrl-`。生成的行写成
   `KeyBinding::new(if cfg!(target_os = "macos") { "cmd-h" } else { "ctrl-h" }, …)` ——
   两个分支都是 `&'static str`，类型成立，编译期折叠，仍然是 80 行。
   `SHORTCUTS` 表第 5 列可以继续存 `secondary-`：它是平台无关的写法，比较时
   两边都会经过 `parse_with_os`，在 macOS 上都落成 `cmd`，语义不变。
3. 加一个 `fn host_os() -> OperatingSystem`（`cfg!(target_os = "macos")` 二选一），
   `snippets::free` 与 `snippet_pane::taken_by` 改用 `Keystroke::parse_with_os(s, host_os())`，
   签名不用动。
4. `snippets::bindings` / `scripts::bindings` 增加 `cx: &App` 参数，内部改用
   `KeyBinding::load(keystroke, Box::new(action), None, false, None, cx)`；
   调用点 `menus.rs:300/304` 传 `&*cx`。`free()` 的 `parse` 失败仍然要拦下来 ——
   现在 `load` 返回 `Result`，正好把 `.expect()` 换成 `filter`。
5. 三处测试：`snippet_pane.rs:884` 的 `fn key(text: &str)` 改成 `&'static str`；
   `snippets.rs:158/164` 用 `parse_with_os(&keystroke, host_os())`。

**判据**：`cargo test --workspace` 全绿；`every_binding_parses` 通过；
`cargo run` 启动不 panic；⌘S / ⌘F / ⌘, 在真机上按下去有反应。

### S2 行为回归（不改代码，只验）

按 §4.4 的表逐条走一遍，重点三条：

- 主编辑器：滚到底再滚，滚轮不该跑到别处；触控板到边应该有 bounce。
- 检查器里的警告表：表横向滚到底后，纵向滚轮是否还能滚外层（预期：不能了）。
- 查找结果表：Home/End、列宽双击。

结论如果是「不接受」，最小改动是给对应容器加 `.scroll_bounce(false)` /
`.scroll_wheel(false)`，不需要改框架版本。

### S3 记账

- `CHANGELOG.md`：一条「工程」——`gpui-kit` 0.7.3 → 0.8.1，工具链固定 1.99.1，
  键位生成器改 `cmd`/`ctrl`。
- `docs/EVALUATION.md` §2 的版本号与依赖树，§4.1 的风险条目（从「运行期 panic」
  改成「编译期错误」，因为 `KeyBinding::new` 现在只收 `&'static str`）。
- `docs/PLAN.md` 已知缺口：`test-support` 那条按 §4.5 的新理由改写。

**工作量**：S1 半天，S2 半天到一天（12 处滚动容器 + 两张表），S3 一小时。
合计 1–1.5 天，不含真机手感确认。

---

## 6. 复现

```bash
# 0.8.1 + 1.96.1：依赖自身编译失败
cp -a . /tmp/t && cd /tmp/t
sed -i '' 's/0\.7\.3/0.8.1/' Cargo.toml
cargo check --workspace --all-targets

# 0.8.1 + 1.99.1：本仓 7 处错误，全在 keystroke
cargo +1.99.1 check --workspace --all-targets

# =0.8.0 + 1.96.1：零改动通过（注意 `=`，否则解析到 0.8.1）
#   gpui-kit = { version = "=0.8.0", default-features = true }
cargo test --workspace

# test-support：0.8.1 上可用（依赖 cocoa 钉版，见 §4.5）
cargo test -p subtext-app --features ui-tests --test ui
```

---

## 7. 实施

按 §5 落地，与本篇评估一起提交：

| 文件 | 改动 |
| --- | --- |
| `rust-toolchain.toml` | 新增，`channel = "1.99.1"` |
| `Cargo.toml` | `gpui-kit` 0.7.3 → 0.8.1 |
| `tools/generate_menu.py` | 新增 `secondary!` 宏；`binding_string` 多一个参数；生成的测试改用 `parse_with_os` |
| `crates/subtext-app/src/keymap.rs` | 重新生成：`KeyBinding::new(secondary!("", "h"), …)` |
| `crates/subtext-app/src/lib.rs` | 新增 `host_os()` |
| `snippets.rs` / `scripts.rs` / `snippet_pane.rs` / `menus.rs` | 动态键位改走 `KeyBinding::load`；`parse` → `parse_with_os` |

与 §5 的一处偏离：S1 第 2 步原本打算把 `if cfg!` 内联进 75 条 `KeyBinding::new`。
那样每次调用会被 rustfmt 拆成七行，整个文件从 717 行长到 1100 行。改成宏之后
一条绑定仍然是一行，读起来还是那张 storyboard 表。

判据（全部通过）：`cargo fmt --all --check`、`cargo test --workspace` 738 项、
`cargo test -p subtext-app --features ui-tests --test ui`、`cargo clippy --workspace
--all-targets -- -D warnings`。
