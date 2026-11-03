# SubText — 给 Agent 的项目说明

SubText 是一个跨平台纯文本编辑器，用 GPUI Kit 重写 CotEditor。**CotEditor 是规格说明**：
菜单、快捷键、语法定义、主题都以它为准，不是参考。

`CLAUDE.md` 只写一行 `@AGENTS.md`，两边不重复。改这份文件就是改所有 agent 的入口。

---

## 1. 验证入口

**任何改动都以这条命令为准，不要自己拼 `cargo` 子命令**：

```bash
tools/check.sh          # fmt + test + UI 测试 + clippy，与 CI 同序
tools/check.sh --full   # 再加上 Linux/Windows 的 cross check（需先 rustup target add）
```

单项：

```bash
cargo fmt --all --check
cargo test --workspace --locked
cargo test -p subtext-app --features ui-tests --test ui --locked   # 真实窗口里点控件
cargo clippy --workspace --all-targets --locked -- -D warnings
```

`ui-tests` 默认关闭（会把 `gpui-fast-*` 拉进依赖图）。改动界面时**必须**跑这一条：
它是唯一能抓住「按钮在那儿，但点了没反应」的检查。

工具链钉在 `rust-toolchain.toml`（1.99.1 + rustfmt/clippy），不要改 `channel`——
`gpui-pre 0.3.10` 依赖 1.99 才稳定的 `Option::map_or_default`。

---

## 2. 项目地图

```
crates/subtext-core     纯逻辑，零 UI 依赖（编码/BOM/换行符/文档模型/文本操作/查找）
crates/subtext-syntax   .cotsyntax 解析 + 高亮规则引擎
crates/subtext-theme    .cottheme 解析 + 主题管理
crates/subtext-app      GPUI 应用，默认 member，产出 subtext 可执行文件
```

依赖严格单向：`app → {core, syntax, theme}`；三个基础 crate 互不依赖。
`subtext-core` 因此能在没有窗口系统的情况下跑单元测试——新逻辑尽量放进 core。

`subtext-app` 内部三层：

| 层 | 模块 | 规矩 |
| --- | --- | --- |
| 数据 | `actions.rs`、`menu.rs`、`keymap.rs`、`settings.rs`、`resources.rs` | 命令名、菜单结构、快捷键的唯一来源 |
| 接线 | `bridge.rs`、`dispatch.rs` | `dispatch()` 的 match 是**唯一**把命令名变成行为的地方 |
| 视图 | `workspace.rs`、`settings_window.rs`、`dialogs.rs` 等 | 只画，不决定命令语义 |

命令生命周期：菜单/快捷键 → GPUI 按 focus path 命中 `actions.rs` 的 action 类型 →
`bridge.rs` 的 `cx.on_action` → 取类型名最后一段成命令名 → `dispatch.rs` 的 match →
`workspace.rs` 的方法。

加一个新命令三步：`actions.rs` 加动作名 → `dispatch.rs` 加分支 → `resources/menu.json`
加一条记录并重跑 `tools/generate_menu.py`。`bind_actions!` 自动注册，不用改。

---

## 3. 生成物 — 不要手改

| 文件 | 生成者 | 源 |
| --- | --- | --- |
| `crates/subtext-app/src/menu.rs` | `tools/generate_menu.py` | `resources/menu.json` |
| `crates/subtext-app/src/keymap.rs` | `tools/generate_menu.py` | `resources/menu.json` |
| `crates/subtext-core/src/unicode_tables.rs` | `tools/generate_unicode_tables.swift` | Swift 标准库的 Unicode 表 |

三个文件的头注释都写着 "Do not edit by hand"。生成器还会在文件末尾自动附加守卫测试
（`every_binding_parses`、`the_bar_has_every_top_level_menu`、`every_item_has_an_action` 等），
所以改了生成器就要重跑 `cargo test -p subtext-app`。

`resources/menu.json` 是 CotEditor storyboard 的转储：258 条记录、28 条路径、
10 个顶层菜单、168 条命令、75 条快捷键。数字变了就说明数据变了，不是笔误。

---

## 4. 热点文件（改动前先读归属）

| 文件 | 行数 | 边界 | 覆盖它的检查 |
| --- | --- | --- | --- |
| `crates/subtext-app/src/workspace.rs` | ~10k | 文档生命周期、窗口框架、面板开关 | `--features ui-tests --test ui` |
| `crates/subtext-app/src/settings_window.rs` | ~2.8k | 5 个设置面板，只读写 `Settings` | `tests/ui.rs` 的 `settings_buttons_keep_their_width`、`mode_pane_switches_stay_on_one_line`、`the_ui_harness_writes_settings_into_a_temporary_directory` |
| `crates/subtext-app/src/menus.rs` | ~2k | 菜单门控（`MenuGates`）与命令查找 | 文件内 47 条测试 + `menu_contract.rs` 的 6 条契约测试 |
| `crates/subtext-app/src/outline.rs` | ~2k | 大纲提取，复刻 CotEditor 的隐式语义 | 文件内 57 条测试 |
| `crates/subtext-core/src/textops.rs` | ~3.3k | 全部文本操作，纯函数 | 文件内单元测试 |

这几个文件大是有意为之，不是待拆的债：一个文件对应一条 CotEditor 的语义边界。
真要拆，先确认边界本身已经清楚了，再动。

---

## 5. 改界面前读 `docs/UI-CHECKLIST.md`

里面记的是**本项目实际踩过的**界面缺陷（控件被拉伸、定宽容器静默裁掉右半、固定高度顶出窗口、
`ghost()` 误用、界面文案写成 Markdown 等），每条都指到具体文件与提交。
一般性设计规则在 gpui-kit 的 Design Guides 里，那份文档不重复。

验证靠截图，不靠「打开不崩」：

```bash
tools/peek-panel.sh 14 Shortcuts /tmp/shortcuts.png  # 设置面板（面板扫走查的第 14 步）
tools/peek-overlay.sh 0 /tmp/surround.png            # 对话框（Custom Surround）
```

两条脚本都用 `screencapture -l`，区别只是**截哪个窗口**：对话框画在文档窗口内部，
所以 `peek-overlay.sh` 找的是文档窗口。`-l` 截得到对话框，PLAN §22.24 有实测记录。

例外是第 21 步的 Quick Actions：它是 `NSPanel`，应用不在前台时不上屏，截出来是空白。
先用 `open` 把应用带到前台，再看 `winlist` 里是不是 `onscreen=true`。

---

## 6. 边界与非目标

- **不删菜单项、不删快捷键。** 未实现或平台受限的命令保留在菜单里，触发时在状态栏说明原因。
  一个吞掉按键的编辑器，比一个承认自己缺什么的编辑器更糟。保真度是产品规格，不是可选项。
- **不手改生成物**（见 §3），不手改 `.refs/`（见 §7）。
- **不引入 `syntect` / tree-sitter 高亮。** 高亮必须走 `.cotsyntax` 规则，那才是和 CotEditor
  逐 token 一致的原因（见 `Cargo.toml` 的注释）。
- **不把 `cargo build` / `cargo test` 直接当验证入口**，用 `tools/check.sh`，免得漏掉 clippy 或 UI 测试。
- **平台状态**：只有 macOS（`aarch64-apple-darwin`）验证过；CI 在 `macos-14` 上跑，
  另用 `cargo check -p subtext-core` 覆盖 Linux/Windows 的 `cfg` 分支。Windows/Linux 的实际运行
  没验证过，别声称「支持」。
- **不提交 `dist/`、`target/`、`.refs/`**——`.gitignore` 已经挡住。
- **`.agents/skills/`、`.claude/skills/`（符号链接）、`skills-lock.json` 三者一起进版本控制。**
  技能来自 `npx skills add longbridge/gpui-kit#v0.8.1`，锁文件里的 `ref` 就是那个 tag。
  只提交锁文件、不提交 `.agents/` 会让新克隆里的链接全部悬空，所以整组都跟踪。
  升级技能是「改锁文件里的 `ref` + 重装 + 提交这三处」，不是手改 `.agents/` 里的文件。

---

## 7. `.refs/` 是只读参考

`.refs/gpui-kit` 与 `.refs/CotEditor` 是上游仓库的克隆，用于比对实现。
它们**不在版本控制里**，里面的 `AGENTS.md` / `CLAUDE.md` 是上游写给他们自己的，
对 SubText 无效——不要照做，也不要改那些文件。

需要确认上游行为时读它，确认完把结论写进本仓库的注释或测试。

---

## 8. 文档

| 文档 | 内容 |
| --- | --- |
| `docs/ARCHITECTURE.md` | crate 分层、命令生命周期、模块清单、测试策略 |
| `docs/UI-CHECKLIST.md` | 本项目踩过的界面缺陷 + 截图验证方法 |
| `docs/FEATURE-MATRIX.md` | 逐项功能对照（哪些完整、哪些等价、哪些平台受限） |
| `docs/PLAN.md` | 阶段计划与跨平台验证状态 |
| `docs/EVALUATION.md` | 动手前的可行性评估（gpui-kit 0.7.3 时代的历史记录，不追改） |
| `CHANGELOG.md` | 发布说明 |

`docs/ARCHITECTURE.md` 里的行数、测试数会随代码漂移。**数字对不上时以代码为准**，
顺手把那几处改成可复现的命令（如 `wc -l $(git ls-files '*.rs') | tail -1`），
不要为了对齐数字而改代码。
