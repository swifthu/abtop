# abtop — iPhone Compact Mode 设计

**日期**: 2026-06-15
**状态**: Draft
**作用域**: abtop v0.4.8+，单文件 + 单常量变更

## 1. 目标

为超窄屏（iPhone 终端，46 列宽）提供专用布局，将关键信息整合到单页内显示：

- 全局 meta（标题 + 时间 + active/session 计数 + CPU/MEM/load/peak）
- 全局 quota 聚合（MiniMax + Claude，每源 1 行 5h + 7d）
- Session 列表（每 session 3 行 × 7 上限）
- 选中 session 的最近 5 条 U/A 对话
- 底部精简按键提示

**核心约束**（来自用户澄清）：
- **iPhone mode 触发**：窗口宽度 ≤ 46 列 且 高度 ≥ 18 行
- **回落提示**：当宽度 ≤ 46 但高度 < 18 时，连 iPhone mode 都装不下，回落显示统一的 "Terminal too small" 提示（复用现有 too-small 渲染），但消息里明确提示 "iPhone mode needs at least 46×18"
- **其他宽度**：> 46 列走原 narrow/desktop 逻辑（60-99 narrow、≥ 100 desktop）
- 交互形态：单页整合（无 tab 切换），保留 ↑↓ select + Enter jump 到 tmux 的原体验
- 数据源：复用现有 `app.sessions`、`session.chat_messages`、`app.rate_limits`，无新增数据采集
- 视觉层次：4 条 ─ 分隔线带区域名（quota / sessions / chat / footer），让用户在 46 列宽度下仍能快速定位

## 2. 布局（46×35）

```
行 1  abtop v0.4.8     14:32  5↑ 5●
行 2  CPU 38%  MEM 62%  load 2.1  ⚡peak(2h13m)
行 3  ─────────────── quota ───────────────
行 4  mmx 5h 65% ↻2h  7d 88% ↻3d
行 5  cl  5h 35% ↻2h  7d 12% ↻5d
行 6  ──────────── sessions ───────────────
行 7  ►CC abtop      ●Work  82% sonnet4.5
行 8    47m · 24 turns · 1.2M tok
行 9    └─ Edit src/pay.rs
行 10 ►CD predict    ◌Wait  91%⚠ opus[1m]
行 11   2m · 1 turn · 12k tok
行 12   └─ waiting for input
行 13  OC api-serv   ●Exec  22% haiku4.5
行 14   12m · 8 turns · 340k tok
行 15   └─ Bash cargo test
行 16  CC docs       ●Exec  45% sonnet4.5
行 17   5m · 3 turns · 87k tok
行 18   └─ Write docs/setup.md
行 19  CD deploy     ⚡Rate  78% opus4.6
行 20   33m · 19 turns · 980k tok
行 21   └─ rate limited (5h)
行 22  CD tickets    ●Work  31% sonnet4.5
行 23   8m · 5 turns · 240k tok
行 24   └─ Read issues/142.md
行 25  CC release    ◌Wait  55% opus4.6
行 26   15m · 9 turns · 510k tok
行 27   └─ waiting for input
行 28 ─────── abtop · 5 chats ───────────────
行 29 U Add JWT auth to /login endpoint
行 30 A I'll add a verify_token middleware…
行 31 U Tests are failing, fix it
行 32 A Looking at the assertion error in
行 33 U Also update README.md
行 34 ──────────────────────────────────────────
行 35 ↑↓ sel  ↵ jump  / filter  x kill  ? help  q quit
```

**行分配**：
- 行 1-2：meta（2 行）
- 行 3：分隔线（quota 标签，1 行）
- 行 4-5：quota 聚合（mmx 1 行 + claude 1 行）
- 行 6：分隔线（sessions 标签，1 行）
- 行 7-27：session 列表（每 session 3 行 × 7 上限 = 21 行）
- 行 28：分隔线（`<session_name> · 5 chats` 标签，1 行）
- 行 29-33：CHAT 区（5 条 U/A 对话）
- 行 34：分隔线（无标签，1 行）
- 行 35：footer 按键提示

合计：35 行（满）。

## 3. 各区域详细设计

### 3.1 Meta 区（2 行）

```
abtop v0.4.8     14:32  5↑ 5●
CPU 38%  MEM 62%  load 2.1  ⚡peak(2h13m)
```

- 行 1：标题（蓝色加粗）+ 时间 + active↑（绿）+ 总数●
- 行 2：CPU%/MEM%/load/peak-hours 警告
- 复用 `header::draw_header` 已有的渲染逻辑；若空间不够，简化第二行只保留 CPU/MEM/load

### 3.2 Quota 区（2 行）

```
mmx 5h 65% ↻2h  7d 88% ↻3d
cl  5h 35% ↻2h  7d 12% ↻5d
```

- 每源 1 行，每行塞 2 个 bucket（5h + 7d）
- mmx 标签：橙色加粗（与现有 theme.title 同色）
- cl 标签：橙色加粗
- 百分比颜色：< 60% 绿、60-80% 黄、> 80% 红
- ↻X 倒计时单位（h/m）：复用 `format_reset_time()`（已存在于 `quota.rs`）
- 缺失数据：显示 `mmx —  / cl —`

### 3.3 Sessions 区（每 session 3 行 × 7 上限）

#### 第 1 行（主行）
```
►CC abtop      ●Work  82% sonnet4.5
```

- ► 选中标记（仅选中 session 显示，空格替代）
- CC / CD / OC Agent 标签（与现有 theme 颜色一致：CC 橙、CD 蓝、OC 绿）
- project 名（限 8 字符，截断加 `…`）
- 状态：●Work（蓝）/ ◌Wait（蓝灰）/ ⚡Rate（红）/ ✓Done（灰）
- Context%：颜色随值（< 60% 绿、60-80% 黄、> 80% 红）
- 模型短名（含 [1m] 后缀）：复用 `sessions::shorten_model`

#### 第 2 行（stats）
```
  47m · 24 turns · 1.2M tok
```

- 缩进 2 字符
- runtime · turns · tokens（灰色辅助色）
- 复用 `fmt_age()` 和 `fmt_tokens()`

#### 第 3 行（task）
```
  └─ Edit src/pay.rs
```

- 缩进 2 字符 + └─ 前缀
- task 描述（截到 38 字符，加 `…`）
- 灰色辅助色
- 数据源优先级：`session.current_tasks.last()` 优先；空则降级为 `session.first_assistant_text` 前 38 字符

### 3.4 CHAT 区（5 行）

```
U Add JWT auth to /login endpoint
A I'll add a verify_token middleware…
U Tests are failing, fix it
A Looking at the assertion error in
U Also update README.md
```

- 显示**选中 session** 的最近 5 条 U/A 对话（旧→新顺序）
- U 蓝色、A 橙色
- 内容截到 38 字符（约 42 列宽度），超出加 `…`
- 数据源：`session.chat_messages`（已有字段）
- 空数据：显示 `no chat yet`（灰色）

### 3.5 分隔线设计

```
───────────────── quota ──────────────────
────────────── sessions ─────────────────
───────────── abtop · 5 chats ────────────
──────────────────────────────────────────
```

- 样式：`───── <name> ──────────`（dim 灰色，name 居中）
- 4 条分隔线分隔 5 个区域
- chat 分隔线显示**当前选中 session 的 project 名**（限 12 字符，超出截断）
- 总宽度精确 46 列（公式：`left_dashes + 1 + name + 1 + right_dashes = 46`，左右至少 3 dash）

### 3.6 Footer（1 行）

```
↑↓ sel  ↵ jump  / filter  x kill  ? help  q quit
```

- 精简按键提示（6 个键：select / jump / filter / kill / help / quit）
- 键字符蓝色，标签主前景色
- 仅保留最常用键，c（config）/ v（view）/ X（kill orphan ports） 等次要键省略

## 4. 实现要点

### 4.1 文件改动清单

| 路径 | 类型 | 说明 |
|---|---|---|
| `src/ui/mod.rs` | 修改 | 新增常量 `IPHONE_WIDTH = 46`；在 `draw` 中分发到 `draw_iphone_mode` |
| `src/ui/iphone.rs` | 新增 | 实现 iPhone 布局的 `draw_iphone_mode` 函数 + 单元测试 |

预计代码量：~250 行（含测试 + 注释）。

### 4.2 关键函数签名

```rust
// src/ui/iphone.rs
pub(crate) fn draw_iphone_mode(
    f: &mut ratatui::Frame,
    app: &App,
    area: ratatui::layout::Rect,
    theme: &Theme,
)
```

实现要点：
- 不绘制外边框（节省行数）
- 复用现有 helpers：`fmt_tokens`、`truncate_str`、`grad_at`、`make_gradient`
- `format_reset_time` 改为 `pub(crate)` 并从 `quota.rs` 导出（首选）；若改动过大则在 `iphone.rs` 内联一个简化版本（仅显示 `2h` / `30m` 格式）
- `shorten_model` 改为 `pub(crate)` 并从 `sessions.rs` 导出（必要）

### 4.3 触发逻辑

```rust
// src/ui/mod.rs
pub(crate) const IPHONE_WIDTH: u16 = 46;       // iPhone mode 触发阈值
pub(crate) const IPHONE_MIN_HEIGHT: u16 = 18;  // iPhone mode 最低高度

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = area.width;
    let h = area.height;

    // ... 现有背景填充 ...

    // iPhone mode 必须在 too small 检查之前：iPhone 宽度 (≤46) 必然 < MIN_WIDTH (60)
    if w <= IPHONE_WIDTH && h >= IPHONE_MIN_HEIGHT {
        draw_iphone_mode(f, app, area, &app.theme);
        draw_overlays(f, app, &app.theme);
        return;
    }

    if w < MIN_WIDTH || h < MIN_HEIGHT {
        // 统一回落提示：现有 too small 渲染，但消息自适应最低尺寸
        // 当 w ≤ 46 && h < IPHONE_MIN_HEIGHT 时显示 "iPhone mode needs 46×18"
        // 当 60 > w > 46 时显示 "Narrow mode needs 60×18"
        draw_too_small(f, area, &app.theme, w, h);
        return;
    }

    if w < DESKTOP_WIDTH {
        draw_narrow(f, app, area, theme);
        draw_overlays(f, app, &app.theme);
        return;
    }

    // 现有 desktop 路径
    let layout = desktop_layout(app, area);
    // ... 现有代码不变 ...
}
```

**`draw_too_small` 改动**：根据当前宽度自动选择推荐的目标模式提示：

```rust
fn draw_too_small(f: &mut Frame, area: Rect, theme: &Theme, w: u16, h: u16) {
    let (target_w, target_h, target_label) = if w <= IPHONE_WIDTH {
        (IPHONE_WIDTH, IPHONE_MIN_HEIGHT, "iphone mode")
    } else {
        (MIN_WIDTH, MIN_HEIGHT, "narrow mode")
    };
    // ... 渲染 "Terminal too small" + 当前 w/h + "Need at least WxH for <target_label>"
}
```

### 4.4 边界条件

| 场景 | 行为 |
|---|---|
| 0 session | CHAT 区显示 "no chat yet"；sessions 区不绘制 |
| session 数 > 7 | 只显示前 7 个 |
| session 数 1-7 | 全部显示，下方留空（不补 padding） |
| `app.rate_limits` 为空 | quota 区显示 `mmx —  / cl —` |
| 选中 session 无 `chat_messages` | CHAT 区显示 "no chat yet" |
| 宽度 ≤ 46 且 高度 < 18 | 回落显示 "iPhone mode needs at least 46×18"（复用 too-small 渲染） |
| 宽度 47-59 且 高度 < 18 | 回落显示 "Narrow mode needs at least 60×18" |
| iPhone mode 高度 18-34 | session 区按 3 行/session 自动缩减到剩余空间，footer/chat/分隔线全部保留 |
| iPhone mode 高度 ≥ 35 | 满布局（7 sessions + 5 chat + 4 分隔线） |
| 选中 index 超出可见范围 | 自动 clamp 到第一个 visible session（现有 `clamp_selection_to_visible` 逻辑） |

### 4.5 渲染性能

- 每 tick 重绘整个 46×35 frame
- 复用现有 braille_bar / meter_bar / grad_at 等无分配 helpers
- CHAT 截断后字符串 ≤ 50 字符，无堆分配热点
- 实测目标：单次 draw < 1ms（远低于 2s tick 间隔）

## 5. 测试策略

### 5.1 单元测试（`src/ui/iphone.rs` 同文件 `#[cfg(test)] mod tests`）

使用 ratatui `TestBackend(46, 35)` 渲染后断言文本包含：

```rust
#[test]
fn iphone_mode_renders_all_sections() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    crate::demo::populate_demo(&mut app);
    let backend = TestBackend::new(46, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &app)).unwrap();
    let text = format!("{}", terminal.backend());

    assert!(text.contains("abtop v"), "meta 标题");
    assert!(text.contains("mmx"), "quota mmx 标签");
    assert!(text.contains("cl "), "quota claude 标签");
    assert!(text.contains("quota"), "分隔线标签");
    assert!(text.contains("sessions"), "分隔线标签");
    assert!(text.contains("chats"), "CHAT 分隔线标签");
    assert!(text.contains("↑↓"), "footer 按键");
    assert!(text.contains("q quit"), "footer 按键");
    assert!(text.contains("U") || text.contains("A"), "CHAT U/A 标记");
}

#[test]
fn iphone_mode_caps_at_7_sessions() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    // 塞 10 个 session
    for _ in 0..10 {
        // ... 添加 session ...
    }
    // 渲染后断言：只渲染 7 个 session 的 project name
}

#[test]
fn iphone_mode_no_chat_shows_placeholder() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    crate::demo::populate_demo(&mut app);
    // 清空选中 session 的 chat_messages
    app.sessions[app.selected].chat_messages.clear();
    // 渲染后断言：包含 "no chat yet"
}

#[test]
fn iphone_mode_zero_sessions_no_panic() {
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    // sessions 为空，渲染不应 panic
}

#[test]
fn iphone_mode_separator_centers_section_name() {
    let backend = TestBackend::new(46, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
    crate::demo::populate_demo(&mut app);
    terminal.draw(|f| draw(f, &app)).unwrap();
    let text = format!("{}", terminal.backend());
    // 验证 quota/sessions/chats 分隔线确实包含 name
    assert!(text.contains(" quota "));
    assert!(text.contains(" sessions "));
    assert!(text.contains("chats"));
}
```

### 5.2 集成验证

- `cargo build` 通过
- `cargo test` 全部通过
- `cargo clippy -- -D warnings` 无警告
- 手动：
  - 真机 iPhone ssh 进去 abtop（46×35）
  - 切换 theme（catppuccin / dracula / nord）验证可读性
  - 多个 session（< 7 和 > 7）切换验证
  - 切换选中 session 验证 CHAT 区跟随更新
  - 按 ↵ 验证 jump 到 tmux pane 不受影响

## 6. 兼容性 / 不破坏现有

**触发顺序**（`src/ui/mod.rs::draw` 中）：

```rust
// 优先级 1：iPhone mode（必须在 too small 之前，宽度 ≤46 必然 < MIN_WIDTH 60）
if w <= IPHONE_WIDTH && h >= IPHONE_MIN_HEIGHT {
    draw_iphone_mode(f, app, area, &app.theme);
    draw_overlays(f, app, &app.theme);
    return;
}

// 优先级 2：回落提示（统一处理：iPhone 装不下 / narrow 装不下）
if w < MIN_WIDTH || h < MIN_HEIGHT {
    draw_too_small(f, area, &app.theme, w, h);
    return;
}

// 优先级 3：narrow
if w < DESKTOP_WIDTH {
    draw_narrow(f, app, area, theme);
    draw_overlays(f, app, &app.theme);
    return;
}

// 优先级 4：desktop
let layout = desktop_layout(app, area);
// ... 现有代码不变 ...
```

**宽度分支矩阵**（h ≥ 18 时）：

| 宽度范围 | 行为 |
|---|---|
| ≤ 46 | **iPhone mode（新增）** |
| 47-59 | "Narrow mode needs 60×18" 提示 |
| 60-99 | narrow 模式（不变） |
| ≥ 100 | desktop 模式（不变） |

**高度 < 18 时**（任意宽度）：

| 宽度范围 | 行为 |
|---|---|
| ≤ 46 | "iPhone mode needs 46×18" 提示 |
| 47-59 | "Narrow mode needs 60×18" 提示 |
| 60-99 | "Narrow mode needs 60×18" 提示 |
| ≥ 100 | "Desktop mode needs 100×18" 提示（扩展现有 too small 提示） |

iPhone mode 完全替代 47-59 区域的"无法使用"状态——这是有意设计：iPhone 用户从 "无法使用" 升级到 "完整可用"。

**不破坏**：60-99 列的 narrow 模式、≥ 100 列的 desktop 模式、现有交互键（↑↓/Enter/x/?/q）、panel 渲染逻辑全部不变。

## 7. 风险与回滚

- **风险 1**：选中 session 无 chat 数据时，CHAT 区显示空 → 显示 "no chat yet" 占位（不 panic）
- **风险 2**：46 列宽度下，长 project 名截断可能让用户困惑 → 截断时加 `…` 提示
- **风险 3**：iPhone mode 没有窄模式（60-99）的 tab 灵活性 → 是有意的简化，UI 文档说明
- **回滚**：所有改动都是新增 `iphone.rs` + `mod.rs` 单点分发，revert 一次 commit 即可完全恢复

## 8. 不做的事（YAGNI）

- ❌ 不支持 47-59 之间的中间宽度（仍走 narrow）
- ❌ 不在 iPhone mode 下绘制 panel borders（节省行数）
- ❌ 不实现 chat 滚动（5 条上限固定）
- ❌ 不做 iPhone landscape 适配（landscape 宽度 > 46 走 desktop）
- ❌ 不改 desktop 任何现有路径
- ❌ 不重构 narrow 模式

## 9. 开放问题

无。所有设计决策已与用户对齐。
