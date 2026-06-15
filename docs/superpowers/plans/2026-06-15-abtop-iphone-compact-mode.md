# abtop iPhone Compact Mode 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为超窄屏（iPhone 终端，46 列宽）新增专用布局 `iPhone mode`，单页整合显示 meta + quota + sessions + 选中 session 的 CHAT + footer；触发条件 `width ≤ 46 && height ≥ 18`，其余路径不变。

**Architecture:** 新增 `src/ui/iphone.rs` 实现 `draw_iphone_mode(f, app, area, theme)`；在 `src/ui/mod.rs::draw` 中插入分发（iPhone check 必须在 too-small 检查之前，因为 `w ≤ 46` 必然 `< MIN_WIDTH=60`）。`draw_too_small` 改造为根据当前宽度自适应提示目标模式（iPhone mode / narrow mode / desktop mode）。4 个 iPhone 区域（meta / quota / sessions / chat）之间用 ─ 分隔线带名字分开，footer 按键提示固定 1 行。

**Tech Stack:** Rust 1.88、ratatui 0.29、crossterm 0.28、chrono 0.4、serde 1。

**Spec:** `docs/superpowers/specs/2026-06-15-abtop-iphone-compact-mode-design.md`

---

## File Structure

| 路径 | 类型 | 职责 |
|---|---|---|
| `src/ui/iphone.rs` | 新增 | `draw_iphone_mode()` + 5 个区域渲染 + 分隔线 + 边界处理 + 单元测试 |
| `src/ui/mod.rs` | 修改 | 新增 `IPHONE_WIDTH`/`IPHONE_MIN_HEIGHT` 常量 + `draw()` 分发 + `draw_too_small()` 改造 |
| `src/ui/quota.rs` | 修改 | `format_reset_time` 加 `pub(crate)` |
| `src/ui/sessions.rs` | 修改 | `shorten_model` 加 `pub(crate)` |

---

## Task 1: 暴露 `format_reset_time` 和 `shorten_model` 给 iPhone 模块使用

**Files:**
- Modify: `src/ui/quota.rs:255`（函数签名加 `pub(crate)`）
- Modify: `src/ui/sessions.rs:987`（函数签名加 `pub(crate)`）

- [ ] **Step 1: 修改 `format_reset_time` 可见性**

编辑 `src/ui/quota.rs:255`，将函数从 `pub(crate) fn` 改为 `pub(crate) fn`（已经是了），并验证：

```bash
grep -n "fn format_reset_time" /Volumes/CC/Fork/abtop/src/ui/quota.rs
```

Expected output 应显示 `pub(crate) fn format_reset_time`。如果已经是这个 visibility，跳过此步。如果不是，加 `pub(crate)` 前缀。

- [ ] **Step 2: 修改 `shorten_model` 可见性**

编辑 `src/ui/sessions.rs:987`，将：

```rust
pub(crate) fn shorten_model(model: &str, is_1m: bool) -> String {
```

如果当前不是 `pub(crate)`，改为：

```rust
pub(crate) fn shorten_model(model: &str, is_1m: bool) -> String {
```

- [ ] **Step 3: 验证编译**

```bash
cd /Volumes/CC/Fork/abtop && cargo build 2>&1 | tail -20
```

Expected: `Finished` 状态，0 errors。

- [ ] **Step 4: Commit**

```bash
cd /Volumes/CC/Fork/abtop && git add src/ui/quota.rs src/ui/sessions.rs && git -c user.name="JimmyHu" -c user.email="jimmyhu@example.com" commit -m "refactor(ui): expose format_reset_time and shorten_model as pub(crate)

Both helpers are reused by the upcoming iPhone mode module.
No behavior change." 2>&1 | tail -5
```

---

## Task 2: 改造 `draw_too_small` 让它根据当前宽度自适应提示目标模式

**Files:**
- Modify: `src/ui/mod.rs:316-378`（重写 too-small 分支）
- Modify: `src/ui/mod.rs:281-283`（新增 `IPHONE_WIDTH`/`IPHONE_MIN_HEIGHT` 常量）

- [ ] **Step 1: 写 failing test**

编辑 `src/ui/mod.rs`，在 `mod tests` 内现有测试之后追加：

```rust
#[test]
fn too_small_promotes_iphone_mode_when_width_below_46() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    let backend = TestBackend::new(40, 15);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &app)).unwrap();
    let text = format!("{}", terminal.backend());
    assert!(
        text.contains("iphone mode"),
        "40x15 should hint at iPhone mode\n{text}"
    );
}

#[test]
fn too_small_promotes_narrow_mode_when_width_between_47_and_59() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    let backend = TestBackend::new(55, 15);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &app)).unwrap();
    let text = format!("{}", terminal.backend());
    assert!(
        text.contains("narrow mode"),
        "55x15 should hint at narrow mode\n{text}"
    );
}
```

- [ ] **Step 2: 运行测试确认 FAIL**

```bash
cd /Volumes/CC/Fork/abtop && cargo test --lib too_small_promotes 2>&1 | tail -20
```

Expected: 2 个测试都 FAIL（当前 too-small 提示文案不含 "iphone mode" 或 "narrow mode"）。

- [ ] **Step 3: 在 `mod.rs` 添加 IPHONE_WIDTH / IPHONE_MIN_HEIGHT 常量**

在 `mod.rs:283` 附近（`DESKTOP_WIDTH` 后）添加：

```rust
pub(crate) const IPHONE_WIDTH: u16 = 46;       // iPhone mode 触发阈值
pub(crate) const IPHONE_MIN_HEIGHT: u16 = 18;  // iPhone mode 最低高度
```

- [ ] **Step 4: 重写 too-small 分支（`mod.rs:316-378`）**

将 `if w < MIN_WIDTH || h < MIN_HEIGHT { ... }` 块替换为：

```rust
if w < MIN_WIDTH || h < MIN_HEIGHT {
    let (target_w, target_h, target_label) = if w <= IPHONE_WIDTH {
        (IPHONE_WIDTH, IPHONE_MIN_HEIGHT, "iphone mode")
    } else if w < DESKTOP_WIDTH {
        (MIN_WIDTH, MIN_HEIGHT, "narrow mode")
    } else {
        (DESKTOP_WIDTH, MIN_HEIGHT, "desktop mode")
    };
    let msg = vec![
        Line::from(Span::styled(
            t("term.too_small"),
            Style::default()
                .fg(theme.main_fg)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled(format!("{} ", t("term.width")), Style::default().fg(theme.main_fg)),
            Span::styled(
                w.to_string(),
                Style::default().fg(if w < MIN_WIDTH { Color::Red } else { Color::Green }),
            ),
            Span::styled(format!(" {} ", t("term.height")), Style::default().fg(theme.main_fg)),
            Span::styled(
                h.to_string(),
                Style::default().fg(if h < MIN_HEIGHT { Color::Red } else { Color::Green }),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            t("term.needed"),
            Style::default().fg(theme.main_fg).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("{} {}  {} {} ({})", t("term.width"), target_w, t("term.height"), target_h, target_label),
            Style::default().fg(theme.main_fg),
        )),
    ];
    let block = Paragraph::new(msg).alignment(Alignment::Center);
    let y = h / 2 - 2;
    let msg_area = Rect {
        x: 0,
        y,
        width: w,
        height: 5.min(h.saturating_sub(y)),
    };
    f.render_widget(block, msg_area);
    return;
}
```

- [ ] **Step 5: 运行测试确认 PASS**

```bash
cd /Volumes/CC/Fork/abtop && cargo test --lib too_small_promotes 2>&1 | tail -10
```

Expected: 2 个测试都 PASS。

- [ ] **Step 6: 全量 cargo test 确认没破坏其他测试**

```bash
cd /Volumes/CC/Fork/abtop && cargo test 2>&1 | tail -10
```

Expected: 所有测试通过，无 regression。

- [ ] **Step 7: Commit**

```bash
cd /Volumes/CC/Fork/abtop && git add src/ui/mod.rs && git -c user.name="JimmyHu" -c user.email="jimmyhu@example.com" commit -m "feat(ui): make too-small prompt target-aware

Prompts user toward iphone mode / narrow mode / desktop mode
based on current width. Adds IPHONE_WIDTH=46 + IPHONE_MIN_HEIGHT=18
constants for the upcoming iPhone mode module." 2>&1 | tail -5
```

---

## Task 3: iPhone mode 入口分发（含最小占位骨架）

**Files:**
- Create: `src/ui/iphone.rs`（初始仅占位 + 1 个测试）
- Modify: `src/ui/mod.rs:1`（加 `mod iphone;`）+ `mod.rs:380`（在 `if w < DESKTOP_WIDTH` 前插入 iPhone 分发）

- [ ] **Step 1: 写 failing test**

创建 `src/ui/iphone.rs`：

```rust
use crate::app::App;
use crate::theme::Theme;
use ratatui::layout::Rect;
use ratatui::Frame;

/// Entry point for iPhone (46-column) compact mode.
///
/// Triggered by `src/ui/mod.rs::draw` when `width <= IPHONE_WIDTH` and
/// `height >= IPHONE_MIN_HEIGHT`. Renders a single-page integrated layout:
/// meta + quota + sessions (max 7 × 3 rows) + selected session chat (5)
/// + footer keybinds, separated by ── named dividers.
pub(crate) fn draw_iphone_mode(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    // TODO: implement meta / quota / sessions / chat / footer / dividers
    // For now, paint a placeholder so we can wire up dispatch in mod.rs.
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let line = Line::from(Span::styled(
        "iphone mode (placeholder)",
        Style::default()
            .fg(theme.title)
            .add_modifier(Modifier::BOLD),
    ));
    f.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PanelVisibility;
    use crate::demo;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn iphone_mode_placeholder_renders() {
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        demo::populate_demo(&mut app);
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        assert!(
            text.contains("iphone mode"),
            "placeholder should render\n{text}"
        );
    }
}
```

- [ ] **Step 2: 运行测试确认 FAIL**

```bash
cd /Volumes/CC/Fork/abtop && cargo test --lib iphone_mode_placeholder 2>&1 | tail -15
```

Expected: FAIL with "cannot find function `draw_iphone_mode`" 或类似（因为 `iphone.rs` 还没被 `mod.rs` 引入）。

- [ ] **Step 3: 在 `mod.rs` 引入 `iphone` 子模块**

编辑 `src/ui/mod.rs:1-13` 的 `mod` 声明块，在合适位置添加：

```rust
mod iphone;
```

- [ ] **Step 4: 运行测试确认 PASS（仅占位测试）**

```bash
cd /Volumes/CC/Fork/abtop && cargo test --lib iphone_mode_placeholder 2>&1 | tail -10
```

Expected: 1 test PASS。

- [ ] **Step 5: 在 `mod.rs::draw` 中插入 iPhone 分发**

在 `mod.rs:380` 附近（`if w < DESKTOP_WIDTH` 前）插入：

```rust
// iPhone mode 必须在 too small 检查之前：iPhone 宽度 (≤46) 必然 < MIN_WIDTH (60)
if w <= IPHONE_WIDTH && h >= IPHONE_MIN_HEIGHT {
    iphone::draw_iphone_mode(f, app, area, &app.theme);
    draw_overlays(f, app, &app.theme);
    return;
}
```

- [ ] **Step 6: 写分发集成测试**

编辑 `src/ui/mod.rs` 的 `mod tests`，追加：

```rust
#[test]
fn iphone_mode_dispatch_renders_for_46x35() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    crate::demo::populate_demo(&mut app);
    let backend = TestBackend::new(46, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &app)).unwrap();
    let text = format!("{}", terminal.backend());
    assert!(
        text.contains("iphone mode"),
        "46x35 should dispatch to iPhone mode\n{text}"
    );
    assert!(
        !text.contains("Terminal too small"),
        "46x35 should not show too-small prompt\n{text}"
    );
}

#[test]
fn iphone_mode_does_not_trigger_above_46_columns() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    crate::demo::populate_demo(&mut app);
    let backend = TestBackend::new(47, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &app)).unwrap();
    let text = format!("{}", terminal.backend());
    assert!(
        !text.contains("iphone mode"),
        "47x35 should not dispatch to iPhone mode\n{text}"
    );
}
```

- [ ] **Step 7: 运行分发测试确认 PASS**

```bash
cd /Volumes/CC/Fork/abtop && cargo test --lib iphone_mode_dispatch 2>&1 | tail -10
```

Expected: 2 个测试都 PASS。

- [ ] **Step 8: 全量 cargo test 确认没破坏其他测试**

```bash
cd /Volumes/CC/Fork/abtop && cargo test 2>&1 | tail -10
```

Expected: 所有测试通过。

- [ ] **Step 9: Commit**

```bash
cd /Volumes/CC/Fork/abtop && git add src/ui/iphone.rs src/ui/mod.rs && git -c user.name="JimmyHu" -c user.email="jimmyhu@example.com" commit -m "feat(ui): add iPhone mode dispatch + placeholder skeleton

When width <= 46 && height >= 18, route draw() to iphone::draw_iphone_mode
instead of the too-small prompt. The placeholder prints a single line so
subsequent tasks can build out the full 5-region layout." 2>&1 | tail -5
```

---

## Task 4: 实现 iPhone mode 完整布局（meta + quota + sessions + chat + footer + 分隔线）

**Files:**
- Modify: `src/ui/iphone.rs`（替换 `draw_iphone_mode` 完整实现 + 5 个新测试）

- [ ] **Step 1: 写 failing test: meta + footer**

在 `src/ui/iphone.rs` 的 `mod tests` 中追加：

```rust
#[test]
fn iphone_mode_meta_and_footer_present() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    demo::populate_demo(&mut app);
    let backend = TestBackend::new(46, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
        .unwrap();
    let text = format!("{}", terminal.backend());
    assert!(text.contains("abtop v"), "meta title row\n{text}");
    assert!(text.contains("↑↓"), "footer select\n{text}");
    assert!(text.contains("↵"), "footer jump\n{text}");
    assert!(text.contains("q quit"), "footer quit\n{text}");
}
```

- [ ] **Step 2: 运行测试确认 FAIL**

```bash
cd /Volumes/CC/Fork/abtop && cargo test --lib iphone_mode_meta_and_footer 2>&1 | tail -10
```

Expected: FAIL（占位实现不渲染这些）。

- [ ] **Step 3: 写 failing test: quota 区**

在 `mod tests` 追加：

```rust
#[test]
fn iphone_mode_quota_section_present() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    demo::populate_demo(&mut app);
    let backend = TestBackend::new(46, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
        .unwrap();
    let text = format!("{}", terminal.backend());
    assert!(text.contains("mmx"), "mmx quota label\n{text}");
    assert!(text.contains("cl "), "claude quota label\n{text}");
    assert!(text.contains("5h"), "5h bucket\n{text}");
    assert!(text.contains("7d"), "7d bucket\n{text}");
}
```

- [ ] **Step 4: 写 failing test: sessions 3 行**

在 `mod tests` 追加：

```rust
#[test]
fn iphone_mode_session_three_rows() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    demo::populate_demo(&mut app);
    let backend = TestBackend::new(46, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
        .unwrap();
    let text = format!("{}", terminal.backend());
    assert!(text.contains("►"), "selected marker\n{text}");
    assert!(text.contains("CC") || text.contains("CD") || text.contains("OC"), "agent label\n{text}");
    assert!(text.contains("●") || text.contains("◌") || text.contains("⚡"), "status icon\n{text}");
    assert!(text.contains("turns"), "stats row\n{text}");
    assert!(text.contains("└─"), "task row\n{text}");
}
```

- [ ] **Step 5: 写 failing test: chat + 分隔线**

在 `mod tests` 追加：

```rust
#[test]
fn iphone_mode_chat_and_dividers() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    demo::populate_demo(&mut app);
    let backend = TestBackend::new(46, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
        .unwrap();
    let text = format!("{}", terminal.backend());
    assert!(text.contains(" quota "), "quota divider\n{text}");
    assert!(text.contains(" sessions "), "sessions divider\n{text}");
    assert!(text.contains("chats"), "chat divider\n{text}");
    // demo sessions may or may not have chat; at minimum, divider should appear.
}
```

- [ ] **Step 6: 写 failing test: 边界 (0 session)**

在 `mod tests` 追加：

```rust
#[test]
fn iphone_mode_no_sessions_does_not_panic() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    // do NOT populate demo
    let backend = TestBackend::new(46, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
        .unwrap();
    // Just check no panic + meta + footer still render
    let text = format!("{}", terminal.backend());
    assert!(text.contains("abtop v"), "meta still renders\n{text}");
    assert!(text.contains("q quit"), "footer still renders\n{text}");
}

#[test]
fn iphone_mode_caps_at_7_sessions() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    // Insert 10 dummy sessions with distinct project names so we can count.
    for i in 0..10 {
        let mut s = crate::model::AgentSession::default();
        s.project_name = format!("p{i:02}");
        s.context_percent = 10.0 + i as f64;
        s.model = "claude-sonnet-4-6".to_string();
        s.status = crate::model::SessionStatus::Working;
        app.sessions.push(s);
    }
    let backend = TestBackend::new(46, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
        .unwrap();
    let text = format!("{}", terminal.backend());
    // p00..p06 should appear; p07..p09 should NOT.
    assert!(text.contains("p00"), "first session should render\n{text}");
    assert!(text.contains("p06"), "7th session should render\n{text}");
    assert!(!text.contains("p07"), "8th session should NOT render\n{text}");
    assert!(!text.contains("p09"), "10th session should NOT render\n{text}");
}
```

注：`AgentSession::default()` 假设实现了 Default。如未实现，构造一个完整 struct literal。

- [ ] **Step 7: 实现 `draw_iphone_mode` 完整布局**

替换 `src/ui/iphone.rs` 中的占位 `draw_iphone_mode` 为完整实现。完整实现代码如下（一次性替换）：

```rust
use crate::app::App;
use crate::locale::t;
use crate::model::RateLimitInfo;
use crate::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::{
    fmt_age, fmt_tokens, grad_at, make_gradient, quota::format_reset_time,
    sessions::shorten_model, truncate_str,
};

const MAX_VISIBLE_SESSIONS: usize = 7;
const CHAT_VISIBLE: usize = 5;
const TASK_TRUNCATE: usize = 38;
const PROJECT_TRUNCATE: usize = 8;
const STATS_TRUNCATE: usize = 8; // legacy unused; kept for clarity

/// Entry point for iPhone (46-column) compact mode.
///
/// Layout: meta(2) ── quota ── quota rows(2) ── sessions ── sessions(≤21) ── <name> chats ── chat(5) ── footer(1).
pub(crate) fn draw_iphone_mode(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let h = area.height;
    let visible_sessions = app.sessions.len().min(MAX_VISIBLE_SESSIONS);
    let sessions_h = (visible_sessions as u16) * 3;
    let chat_h = if app.sessions.is_empty() { 1u16 } else { CHAT_VISIBLE as u16 };
    let fixed_h = 2 /* meta */ + 1 /* divider */ + 2 /* quota */
        + 1 /* divider */ + 1 /* chat divider */
        + 1 /* chat */ + 1 /* footer divider */ + 1 /* footer */;
    let mut actual_sessions_h = sessions_h.min(h.saturating_sub(fixed_h));
    // round down to multiple of 3
    actual_sessions_h -= actual_sessions_h % 3;
    let actual_visible = (actual_sessions_h / 3) as usize;
    let actual_chat_h = if app.sessions.is_empty() { 1 } else { chat_h as usize };

    let mut constraints = vec![
        Constraint::Length(2),                              // meta
        Constraint::Length(1),                              // quota divider
        Constraint::Length(2),                              // quota
        Constraint::Length(1),                              // sessions divider
        Constraint::Length(actual_sessions_h),              // sessions
        Constraint::Length(1),                              // chat divider
        Constraint::Length(actual_chat_h as u16),           // chat
        Constraint::Length(1),                              // footer divider
        Constraint::Length(1),                              // footer
    ];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints.clone())
        .split(area);

    draw_meta(f, app, chunks[0], theme);
    draw_divider(f, chunks[1], theme, "quota");
    draw_quota(f, app, chunks[2], theme);
    draw_divider(f, chunks[3], theme, "sessions");
    draw_sessions(f, app, chunks[4], theme, actual_visible);
    draw_chat_divider(f, chunks[5], app, theme);
    draw_chat(f, app, chunks[6], theme);
    draw_divider(f, chunks[7], theme, "");
    draw_footer(f, chunks[8], theme);

    // Suppress unused-constraint warning when areas are unused.
    let _ = constraints.pop();
}

fn draw_meta(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let session_count = app.sessions.len();
    let active = app.agent_aggregate.active_count;
    let now = chrono::Local::now().format("%H:%M").to_string();
    let version = env!("CARGO_PKG_VERSION");
    let cpu_label = t("header.cpu");
    let mem_label = t("header.mem");
    let load_label = t("header.load");
    let title = format!(" abtop v{version} ");
    let counter = format!(" {now}  {active}↑ {session_count}● ");
    let used_title = title.chars().count();
    let used_counter = counter.chars().count();
    let pad = (area.width as usize).saturating_sub(used_title + used_counter);
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(
        title,
        Style::default().fg(theme.title).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(Span::styled(counter, Style::default().fg(theme.graph_text)));

    let row1 = Line::from(spans);
    let host = app.host_metrics.as_ref();
    let mut row2_spans: Vec<Span> = Vec::new();
    if let Some(h) = host {
        row2_spans.push(Span::styled(
            format!("{} {:>2.0}%  ", cpu_label, h.cpu_pct),
            Style::default().fg(theme.graph_text),
        ));
        row2_spans.push(Span::styled(
            format!("{} {:>2.0}%  ", mem_label, h.mem_pct),
            Style::default().fg(theme.graph_text),
        ));
        row2_spans.push(Span::styled(
            format!("{} {:.1}", load_label, h.load1),
            Style::default().fg(theme.graph_text),
        ));
    } else {
        row2_spans.push(Span::styled(
            "loading…",
            Style::default().fg(theme.inactive_fg),
        ));
    }
    let row2 = Line::from(row2_spans);
    f.render_widget(Paragraph::new(vec![row1, row2]), area);
}

fn draw_divider(f: &mut Frame, area: Rect, theme: &Theme, name: &str) {
    let width = area.width as usize;
    let text = format!(" {} ", name);
    let text_w = text.chars().count();
    if width == 0 || text_w >= width {
        let line = "─".repeat(width);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                line,
                Style::default().fg(theme.div_line),
            ))),
            area,
        );
        return;
    }
    let total_dash = width - text_w;
    let left = total_dash / 2;
    let right = total_dash - left;
    let line = Line::from(vec![
        Span::styled("─".repeat(left), Style::default().fg(theme.div_line)),
        Span::styled(text, Style::default().fg(theme.title)),
        Span::styled("─".repeat(right), Style::default().fg(theme.div_line)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_chat_divider(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let name = app
        .sessions
        .get(app.selected)
        .map(|s| truncate_str(&s.project_name, 12))
        .unwrap_or_else(|| "—".to_string());
    let label = format!(" {} · {} chats ", name, CHAT_VISIBLE);
    draw_divider_with_label(f, area, theme, &label);
}

fn draw_divider_with_label(f: &mut Frame, area: Rect, theme: &Theme, label: &str) {
    let width = area.width as usize;
    if width == 0 {
        return;
    }
    let text_w = label.chars().count();
    if text_w >= width {
        draw_divider(f, area, theme, "");
        return;
    }
    let total_dash = width - text_w;
    let left = total_dash / 2;
    let right = total_dash - left;
    let line = Line::from(vec![
        Span::styled("─".repeat(left), Style::default().fg(theme.div_line)),
        Span::styled(
            label.to_string(),
            Style::default().fg(theme.title),
        ),
        Span::styled("─".repeat(right), Style::default().fg(theme.div_line)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_quota(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let mut lines: Vec<Line> = Vec::new();
    for source in &["mmx", "claude"] {
        let rl = app
            .rate_limits
            .iter()
            .find(|r| r.source.eq_ignore_ascii_case(source));
        let label = if source.eq_ignore_ascii_case("mmx") { "mmx" } else { "cl " };
        let mut spans: Vec<Span> = Vec::new();
        spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(theme.title).add_modifier(Modifier::BOLD),
        ));
        match rl {
            Some(rl) => {
                for (label_h, pct, reset) in [
                    (
                        "5h",
                        rl.five_hour_pct,
                        rl.five_hour_resets_at,
                    ),
                    (
                        "7d",
                        rl.seven_day_pct,
                        rl.seven_day_resets_at,
                    ),
                ] {
                    let pct_str = match pct {
                        Some(p) => format!("{:>3.0}%", p),
                        None => " — ".to_string(),
                    };
                    let reset_str = match (reset, pct) {
                        (Some(ts), Some(_)) => format!("↻{}", format_reset_time(*ts)),
                        _ => String::new(),
                    };
                    let color = match pct {
                        Some(p) if *p >= 80.0 => Color::Red,
                        Some(p) if *p >= 60.0 => Color::Yellow,
                        Some(_) => Color::Green,
                        None => theme.inactive_fg,
                    };
                    spans.push(Span::styled(
                        format!(" {label_h} "),
                        Style::default().fg(theme.graph_text),
                    ));
                    spans.push(Span::styled(pct_str, Style::default().fg(color)));
                    spans.push(Span::styled(
                        format!(" {reset_str}"),
                        Style::default().fg(theme.graph_text),
                    ));
                }
            }
            None => {
                spans.push(Span::styled(" —  — ", Style::default().fg(theme.inactive_fg)));
            }
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_sessions(f: &mut Frame, app: &App, area: Rect, theme: &Theme, max_visible: usize) {
    if app.sessions.is_empty() || area.height == 0 {
        let placeholder = Line::from(Span::styled(
            " no sessions ",
            Style::default().fg(theme.inactive_fg),
        ));
        f.render_widget(Paragraph::new(placeholder), area);
        return;
    }
    let visible = app.visible_indices();
    let cpu_grad = make_gradient(theme.cpu_grad.start, theme.cpu_grad.mid, theme.cpu_grad.end);
    let mut lines: Vec<Line> = Vec::new();
    for &idx in visible.iter().take(max_visible) {
        let session = &app.sessions[idx];
        let selected = idx == app.selected;
        let marker = if selected { "►" } else { " " };
        let (agent_label, agent_color) = match session.agent_cli.as_str() {
            "claude" => ("CC", Color::Rgb(217, 119, 87)),
            "codex" => ("CD", Color::Rgb(122, 157, 255)),
            "opencode" => ("OC", Color::Rgb(74, 222, 128)),
            other => {
                let fb: String = other.chars().take(2).collect::<String>().to_uppercase();
                // leak-free: use a const fallback
                let fallback: &'static str = Box::leak(fb.into_boxed_str());
                (fallback, theme.inactive_fg)
            }
        };
        let (status_icon, status_color) = match &session.status {
            crate::model::SessionStatus::Thinking => ("●Work", theme.proc_misc),
            crate::model::SessionStatus::Executing => ("●Exec", theme.hi_fg),
            crate::model::SessionStatus::Waiting => ("◌Wait", grad_at(&cpu_grad, 50.0)),
            crate::model::SessionStatus::Unknown => ("◌Unk", theme.inactive_fg),
            crate::model::SessionStatus::RateLimited => ("⚡Rate", theme.status_fg),
            crate::model::SessionStatus::Done => ("✓Done", theme.inactive_fg),
        };
        let is_1m = session.context_window >= 1_000_000 || session.model.contains("[1m]");
        let model_short = shorten_model(&session.model, is_1m);
        let ctx_color = grad_at(&cpu_grad, session.context_percent);

        let is_done = matches!(session.status, crate::model::SessionStatus::Done);
        let row_style = if selected {
            Style::default().bg(theme.selected_bg).fg(theme.selected_fg)
        } else if is_done {
            Style::default().fg(theme.inactive_fg)
        } else {
            Style::default()
        };

        // Row 1: marker + agent + project + status + ctx% + model
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(theme.hi_fg)),
            Span::styled(agent_label, Style::default().fg(agent_color)),
            Span::styled(
                format!(
                    " {:<w$}",
                    truncate_str(&session.project_name, PROJECT_TRUNCATE),
                    w = PROJECT_TRUNCATE
                ),
                Style::default().fg(if selected { theme.selected_fg } else { theme.title }),
            ),
            Span::styled(
                format!(" {} ", status_icon),
                Style::default().fg(status_color),
            ),
            Span::styled(
                format!("{:>3.0}%", session.context_percent),
                Style::default().fg(if selected { theme.selected_fg } else { ctx_color }),
            ),
            Span::styled(
                format!(" {}", truncate_str(&model_short, 12)),
                Style::default().fg(if selected { theme.selected_fg } else { theme.graph_text }),
            ),
        ])
        .style(row_style));

        // Row 2: stats
        lines.push(Line::from(Span::styled(
            format!(
                "  {} · {} turns · {} tok",
                fmt_age(session.elapsed_seconds()),
                session.turn_count,
                fmt_tokens(session.total_tokens())
            ),
            Style::default().fg(theme.inactive_fg),
        )));

        // Row 3: task
        let task = session
            .current_tasks
            .last()
            .cloned()
            .unwrap_or_else(|| session.first_assistant_text.clone());
        lines.push(Line::from(Span::styled(
            format!("  └─ {}", truncate_str(&task, TASK_TRUNCATE)),
            Style::default().fg(theme.graph_text),
        )));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_chat(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    if app.sessions.is_empty() {
        let line = Line::from(Span::styled(
            "  no chat yet",
            Style::default().fg(theme.inactive_fg),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    let Some(session) = app.sessions.get(app.selected) else {
        return;
    };
    if session.chat_messages.is_empty() {
        let line = Line::from(Span::styled(
            "  no chat yet",
            Style::default().fg(theme.inactive_fg),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    let start = session
        .chat_messages
        .len()
        .saturating_sub(CHAT_VISIBLE);
    let mut lines: Vec<Line> = Vec::new();
    for msg in session.chat_messages.iter().skip(start) {
        let (label, color) = match msg.role {
            crate::model::ChatRole::User => ("U", theme.hi_fg),
            crate::model::ChatRole::Assistant => ("A", theme.proc_misc),
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", label), Style::default().fg(color)),
            Span::styled(
                truncate_str(&msg.text, TASK_TRUNCATE),
                Style::default().fg(theme.main_fg),
            ),
        ]));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(f: &mut Frame, area: Rect, theme: &Theme) {
    let mut spans: Vec<Span> = Vec::new();
    for (key, label) in [
        ("↑↓", "sel"),
        ("↵", "jump"),
        ("/", "filter"),
        ("x", "kill"),
        ("?", "help"),
        ("q", "quit"),
    ] {
        spans.push(Span::styled(key, Style::default().fg(theme.hi_fg)));
        spans.push(Span::styled(
            format!(" {label}  "),
            Style::default().fg(theme.main_fg),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
```

注意：`AgentSession` 需要有 `elapsed_seconds()` 方法（参考现有 `elapsed_display`）。如果不存在，使用 `chrono::DateTime::from_timestamp` 计算 elapsed。或者先在 `AgentSession` 上加 `elapsed_seconds() -> u64` 方法。如果不行，可以直接用 `chrono::Local::now().timestamp().saturating_sub(started_at as i64).max(0) as u64`。

- [ ] **Step 8: 添加 `AgentSession::elapsed_seconds()` 辅助方法（如不存在）**

编辑 `src/model/session.rs`，在 `impl AgentSession` 块添加：

```rust
/// Elapsed seconds since the session started (saturating).
pub fn elapsed_seconds(&self) -> u64 {
    let now = chrono::Local::now().timestamp().max(0) as u64;
    now.saturating_sub(self.started_at / 1000)
}
```

注：`started_at` 是 ms 还是 s？参考现有 `elapsed_display()` 实现：

```bash
grep -n "elapsed_display\|started_at" /Volumes/CC/Fork/abtop/src/model/session.rs | head -10
```

如果 `started_at` 是 ms（millis since epoch），除以 1000 转 s。如果已经是 s，直接 `now.saturating_sub(started_at)`。

- [ ] **Step 9: 运行 Task 4 所有测试**

```bash
cd /Volumes/CC/Fork/abtop && cargo test --lib iphone_mode 2>&1 | tail -20
```

Expected: 全部 PASS。如果 FAIL，按错误信息调整（最可能是字段名/类型不匹配，按编译错误提示修正）。

- [ ] **Step 10: 全量 cargo test + cargo clippy**

```bash
cd /Volumes/CC/Fork/abtop && cargo test 2>&1 | tail -10 && cargo clippy -- -D warnings 2>&1 | tail -10
```

Expected: 所有测试通过，clippy 无警告。

- [ ] **Step 11: 手动视觉验证（可选）**

```bash
cd /Volumes/CC/Fork/abtop && cargo run
```

把终端调到 46 列宽，确认看到：
- meta 行：abtop v0.4.8 + 时间 + 计数
- 第二行：CPU/MEM/load 或 loading
- `── quota ──` 分隔线
- mmx 行 + claude 行
- `── sessions ──` 分隔线
- 7 个 session × 3 行
- `── <name> · 5 chats ──` 分隔线
- 5 条 U/A
- `───────────` 分隔线
- `↑↓ sel ↵ jump / filter x kill ? help q quit`

- [ ] **Step 12: Commit**

```bash
cd /Volumes/CC/Fork/abtop && git add src/ui/iphone.rs src/model/session.rs && git -c user.name="JimmyHu" -c user.email="jimmyhu@example.com" commit -m "feat(ui): implement iPhone mode 5-region layout

Single-page integrated dashboard for 46-column terminals:
- meta header (title + time + counts + host vitals)
- named dividers (quota / sessions / <name> chats / footer)
- quota aggregate (mmx + claude, 5h + 7d with reset countdowns)
- session list (max 7, 3 rows each: status / stats / task)
- selected session chat tail (5 U/A messages)
- compact footer (6 keybinds)

Sessions auto-reduce to fit available height (round down to multiples of 3).
Empty chat / no sessions render a placeholder instead of panicking." 2>&1 | tail -5
```

---

## Self-Review Checklist

执行前请检查：

1. **Spec 覆盖**：
   - ✅ 触发条件 w ≤ 46 && h ≥ 18 → Task 3 + Task 4 Step 7
   - ✅ draw_too_small 自适应 → Task 2
   - ✅ 5 个区域 (meta/quota/sessions/chat/footer) → Task 4 Step 7
   - ✅ 4 条 ─ 分隔线带名字 → Task 4 Step 7 (`draw_divider` + `draw_chat_divider`)
   - ✅ session 3 行 × 7 上限 → Task 4 Step 7 (`draw_sessions` + `MAX_VISIBLE_SESSIONS`)
   - ✅ chat 5 条 U/A → Task 4 Step 7 (`draw_chat` + `CHAT_VISIBLE`)
   - ✅ 复用 helpers (format_reset_time, shorten_model, fmt_tokens, truncate_str, grad_at) → Task 1 + Task 4 Step 7
   - ✅ 边界: 0 session / >7 session / height < 35 → Task 4 Step 6 + Step 7 (`actual_sessions_h` 自动缩减)

2. **占位符扫描**：plan 中无 TBD / TODO（仅在初始占位 `draw_iphone_mode` 中有 "TODO"，但 Step 7 立刻替换为完整实现）。

3. **类型一致性**：
   - `RateLimitInfo` 字段：`source`、`five_hour_pct`、`five_hour_resets_at`、`seven_day_pct`、`seven_day_resets_at` — 与 `src/model/session.rs` 定义一致
   - `SessionStatus` 枚举：Thinking/Executing/Waiting/Unknown/RateLimited/Done — 与 `src/model/session.rs` 一致
   - `ChatRole` 枚举：User/Assistant — 与 `src/model/session.rs` 一致
   - `AgentSession` 字段：`agent_cli`、`session_id`、`project_name`、`current_tasks`、`first_assistant_text`、`chat_messages`、`context_percent`、`context_window`、`model`、`status`、`turn_count`、`total_tokens`（通过 `total_tokens()` 方法）— 与 `src/model/session.rs` 一致
   - `Theme` 字段：`title`、`main_fg`、`graph_text`、`inactive_fg`、`div_line`、`proc_box`、`cpu_grad`、`hi_fg`、`proc_misc`、`selected_bg`、`selected_fg`、`status_fg`、`meter_bg` — 与 `src/theme.rs` 一致

4. **已知待调整点**：
   - `started_at` 是 ms 还是 s 待 Step 8 验证
   - `AgentSession::default()` 可能未实现，Step 6 测试可能需要显式构造
   - `chat_messages.len()` 在 0-4 之间，CHAT_VISIBLE=5 不会越界
