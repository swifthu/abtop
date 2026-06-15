use crate::app::App;
use crate::model::{ChatMessage, ChatRole, SessionStatus};
use crate::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{LineGauge, Paragraph};
use ratatui::Frame;

use super::quota::format_reset_time;
use super::sessions::shorten_model;
use super::{braille_graph_multirow, fmt_tokens, grad_at, make_gradient, truncate_str};

/// Maximum number of sessions rendered in the iPhone-mode list.
const MAX_VISIBLE_SESSIONS: usize = 5;
/// Max chars for the rendered task text (`└─ ...`).
const TASK_TRUNCATE: usize = 38;
/// Max chars for the session summary text (row 2), after the `Summary: `
/// prefix and the 2-cell indent.
const SUMMARY_TRUNCATE: usize = 35;
/// Max chars for the project column.
const PROJECT_TRUNCATE: usize = 12;
/// Max chars for the model name shown in the session row 1.
const MODEL_TRUNCATE: usize = 12;
/// Width of each tokens-panel bar (label-relative, in cells). The desktop
/// panel renders at variable width; on iPhone we pin it so the right-hand
/// "Token Rate / graph / Total" column has predictable space.
const TOKENS_BAR_W: u16 = 11;

/// Localized status label with the same icon prefix used on the desktop
/// session panel (e.g. "● Think", "◌ Wait", "✓ Done"). The icon is part
/// of the i18n string so a Chinese-locale user sees "● 思考" without code
/// changes.
pub(crate) fn status_short(status: &SessionStatus) -> String {
    let key = match status {
        SessionStatus::Thinking => "sess.think",
        SessionStatus::Executing => "sess.exec",
        SessionStatus::Waiting => "sess.wait",
        SessionStatus::Unknown => "sess.unknown",
        SessionStatus::RateLimited => "sess.rate",
        SessionStatus::Done => "sess.done",
    };
    crate::locale::t(key)
}

/// Entry point for iPhone (sub-60-column) compact mode.
///
/// Triggered by `src/ui/mod.rs::draw` when `width < MIN_WIDTH` and
/// `height >= IPHONE_MIN_HEIGHT`. Renders a single-page integrated layout:
/// meta + quota + sessions (max 5 × 3 rows) + tokens panel (5 rows) +
/// selected session chat (5) + footer keybinds, separated by ── named
/// dividers.
pub(crate) fn draw_iphone_mode(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let h = area.height;
    let visible_sessions = app.sessions.len().min(MAX_VISIBLE_SESSIONS);
    let sessions_h = (visible_sessions as u16) * 3;
    // The chat panel's only floor is 1 row (so the "no chat yet" placeholder
    // stays single-line when there are no sessions). Above that, the layout
    // gives the chat panel every remaining row, and `draw_chat` then fills
    // it with as many recent messages as fit, bottom-pinned.
    let fixed_h = 2 // meta
        + 1 // quota divider
        + 2 // quota
        + 1 // sessions divider
        + 1 // tokens divider
        + 5 // tokens
        + 1 // chat divider
        + 1 // chat (min)
        + 1 // footer divider
        + 1; // footer
    let mut actual_sessions_h = sessions_h.min(h.saturating_sub(fixed_h));
    // Round down to multiples of 3 so we never show a partial session block.
    actual_sessions_h -= actual_sessions_h % 3;
    let actual_visible = (actual_sessions_h / 3) as usize;
    let actual_chat_h = 1usize;

    let constraints = vec![
        Constraint::Length(2),                       // meta
        Constraint::Length(1),                       // quota divider
        Constraint::Length(2),                       // quota
        Constraint::Length(1),                       // sessions divider
        Constraint::Length(actual_sessions_h),       // sessions
        Constraint::Length(1),                       // tokens divider
        Constraint::Length(5),                       // tokens
        Constraint::Length(1),                       // chat divider
        Constraint::Min(actual_chat_h as u16),       // chat (fills remaining)
        Constraint::Length(1),                       // footer divider
        Constraint::Length(1),                       // footer
    ];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    draw_meta(f, app, chunks[0], theme);
    draw_divider(f, chunks[1], theme, "mmx · quota", theme.cpu_box);
    draw_quota(f, app, chunks[2], theme);
    draw_divider(f, chunks[3], theme, "sessions", theme.proc_box);
    draw_sessions(f, app, chunks[4], theme, actual_visible);
    draw_divider(f, chunks[5], theme, "tokens", theme.cpu_box);
    draw_tokens(f, app, chunks[6], theme);
    draw_chat_divider(f, chunks[7], app, theme);
    draw_chat(f, app, chunks[8], theme);
    draw_divider(f, chunks[9], theme, "", theme.div_line);
    draw_footer(f, chunks[10], theme);
}

/// Row 1: title + time + active↑ + session count
/// Row 2: CPU/MEM/load (when host_metrics is set), or "loading…"
fn draw_meta(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let now = chrono::Local::now().format("%H:%M").to_string();
    let version = env!("CARGO_PKG_VERSION");
    let title = format!(" abtop v{} ", version);
    let active = app.agent_aggregate.active_count;
    let session_count = app.sessions.len();
    let right = format!(" {}  {}↑ {}● ", now, active, session_count);

    let mut row1_spans: Vec<Span> = vec![
        Span::styled(
            title,
            Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];

    // Compute host vitals on the right side of row 1 only when there's space.
    let used1: usize = row1_spans
        .iter()
        .map(|s| s.content.chars().count())
        .sum::<usize>()
        + right.chars().count();
    let width = area.width as usize;
    let pad = width.saturating_sub(used1);
    if pad > 0 {
        row1_spans.push(Span::raw(" ".repeat(pad)));
    }
    row1_spans.push(Span::styled(
        right,
        Style::default().fg(theme.graph_text),
    ));

    let mut lines: Vec<Line> = vec![Line::from(row1_spans)];

    // Row 2: CPU / MEM / load or "loading…"
    let row2_spans: Vec<Span> = if let Some(host) = &app.host_metrics {
        vec![
            Span::styled(" cpu ", Style::default().fg(theme.graph_text)),
            Span::styled(
                format!("{:>2.0}%", host.cpu_pct),
                Style::default().fg(grad_at(
                    &make_gradient(theme.cpu_grad.start, theme.cpu_grad.mid, theme.cpu_grad.end),
                    host.cpu_pct,
                )),
            ),
            Span::styled("  mem ", Style::default().fg(theme.graph_text)),
            Span::styled(
                format!("{:>2.0}%", host.mem_pct),
                Style::default().fg(theme.main_fg),
            ),
            Span::styled("  load ", Style::default().fg(theme.graph_text)),
            Span::styled(
                format!("{:.1}", host.load1),
                Style::default().fg(theme.main_fg),
            ),
        ]
    } else {
        vec![Span::styled(
            " loading… ",
            Style::default().fg(theme.inactive_fg),
        )]
    };
    lines.push(Line::from(row2_spans));

    f.render_widget(Paragraph::new(lines), area);
}

/// Render a divider row with a centered `─ label ─` band.
///
/// `box_color` is the color used for the dashes and the bold label band,
/// matching the desktop panel-box palette. Callers typically pass
/// `theme.cpu_box` (quota / tokens) or `theme.proc_box` (sessions / chat).
/// Pass `theme.div_line` for the bare footer separator.
fn draw_divider(f: &mut Frame, area: Rect, theme: &Theme, label: &str, box_color: Color) {
    let w = area.width as usize;
    let mut spans: Vec<Span> = Vec::new();
    if label.is_empty() {
        spans.push(Span::styled(
            "─".repeat(w),
            Style::default().fg(theme.div_line),
        ));
    } else {
        let band = format!(" {} ", label);
        let band_w = band.chars().count();
        let left = w.saturating_sub(band_w) / 2;
        let right = w.saturating_sub(band_w) - left;
        if left > 0 {
            spans.push(Span::styled(
                "─".repeat(left),
                Style::default().fg(box_color),
            ));
        }
        spans.push(Span::styled(
            band,
            Style::default().fg(box_color).add_modifier(Modifier::BOLD),
        ));
        if right > 0 {
            spans.push(Span::styled(
                "─".repeat(right),
                Style::default().fg(box_color),
            ));
        }
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Divider with a `<session_name> · N chats` label centered.
/// Falls back to `chats` when no session is selected.
fn draw_chat_divider(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let label = if let Some(session) = app.sessions.get(app.selected) {
        let name = if session.project_name.is_empty() {
            "session"
        } else {
            session.project_name.as_str()
        };
        let count = session.chat_messages.len();
        format!(" {} · {} chats ", name, count)
    } else {
        " chats ".to_string()
    };
    draw_divider(f, area, theme, &label, theme.proc_box);
}

/// Render one quota bucket row using ratatui's native `LineGauge` widget.
/// Layout: `5h ` (3) + LineGauge (fills remaining) + " " + time (always two parts,
/// e.g. `in 2h 13m`, prefixed by a leading space). The bar represents
/// **remaining** quota (100% − used%), so a long green bar means a lot of
/// headroom and a short red bar means the bucket is nearly drained. Bar
/// color follows the remaining share: green (>40%) / yellow (20–40%) /
/// red (<20%). The percentage on the gauge label is also the remaining
/// share.
fn quota_bucket_row(
    f: &mut Frame,
    label: &str,
    pct: Option<f64>,
    reset: Option<u64>,
    theme: &Theme,
    area: Rect,
) {
    let label_style = Style::default().fg(theme.title).add_modifier(Modifier::BOLD);
    let reset_style = Style::default().fg(theme.graph_text);

    // Layout: "5h " (3) + LineGauge (fills) + " " + time (" in 2h 13m", 9 chars)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(3),    // "5h "
            Constraint::Min(0),       // LineGauge fills remaining
            Constraint::Length(11),   // " in 6d 12h" — leading space + 9 char time
        ])
        .split(area);

    // Label "5h "
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(format!("{label} "), label_style))),
        chunks[0],
    );

    match pct {
        Some(used) => {
            // Convert "used" share to "remaining" share. The bar's length and
            // color both describe how much is left, not how much is spent.
            let left = (100.0 - used).clamp(0.0, 100.0);
            let color = if left < 20.0 {
                theme.status_fg
            } else if left < 40.0 {
                theme.warning_fg
            } else {
                theme.proc_misc
            };
            let ratio = (left / 100.0).clamp(0.0, 1.0);
            let gauge = LineGauge::default()
                .ratio(ratio)
                .filled_style(Style::default().fg(color))
                .unfilled_style(Style::default().fg(theme.meter_bg))
                .label(format!("{:>3.0}%", left));
            f.render_widget(gauge, chunks[1]);

            // Time text: " in 2h 13m" (leading space + format_reset_time)
            let time_str = reset
                .map(format_reset_time)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "in —".to_string());
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" {time_str}"),
                    reset_style,
                ))),
                chunks[2],
            );
        }
        None => {
            // No usage data: dim gauge + N/A time
            let gauge = LineGauge::default()
                .ratio(0.0)
                .filled_style(Style::default().fg(theme.inactive_fg))
                .unfilled_style(Style::default().fg(theme.meter_bg))
                .label("—");
            f.render_widget(gauge, chunks[1]);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(" in —", reset_style))),
                chunks[2],
            );
        }
    }
}

/// Render two quota rows for the mmx source (5h and 7d) using native LineGauge.
fn draw_quota(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let mmx = app
        .rate_limits
        .iter()
        .find(|r| r.source.eq_ignore_ascii_case("mmx"));
    // Split area vertically into 2 rows
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    quota_bucket_row(
        f,
        "5h",
        mmx.and_then(|r| r.five_hour_pct),
        mmx.and_then(|r| r.five_hour_resets_at),
        theme,
        rows[0],
    );
    quota_bucket_row(
        f,
        "7d",
        mmx.and_then(|r| r.seven_day_pct),
        mmx.and_then(|r| r.seven_day_resets_at),
        theme,
        rows[1],
    );
}

/// Session list: each session takes 3 rows (status header / stats / task).
fn draw_sessions(f: &mut Frame, app: &App, area: Rect, theme: &Theme, max_visible: usize) {
    if area.height == 0 || max_visible == 0 {
        return;
    }
    let visible = app.visible_indices();
    let proc_grad = make_gradient(
        theme.cpu_grad.start,
        theme.cpu_grad.mid,
        theme.cpu_grad.end,
    );

    // Each session gets a 3-row block: status / stats / task.
    let blocks = max_visible.min(visible.len());
    let total_h = (blocks as u16) * 3;
    let constraints: Vec<Constraint> = (0..total_h).map(|_| Constraint::Length(1)).collect();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for i in 0..blocks {
        let session_idx = visible[i];
        let session = &app.sessions[session_idx];
        let row_block = &chunks[(i as u16 * 3) as usize..(i as u16 * 3 + 3) as usize];

        draw_session_row1(f, app, session, row_block[0], theme, &proc_grad, session_idx);
        draw_session_row2(f, app, session, row_block[1], theme);
        draw_session_row3(f, session, row_block[2], theme);
    }
}

/// Row 1: `►CC abtop      ●Work  82% sonnet4.5`
fn draw_session_row1(
    f: &mut Frame,
    app: &App,
    session: &crate::model::AgentSession,
    area: Rect,
    theme: &Theme,
    grad: &[Color; 101],
    session_idx: usize,
) {
    let selected = session_idx == app.selected;
    let marker = if selected { "►" } else { " " };
    let marker_style = if selected {
        Style::default()
            .bg(theme.selected_bg)
            .fg(theme.selected_fg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.hi_fg)
    };

    // Brand colors match the desktop session panel: terracotta / periwinkle / emerald.
    // iPhone layout lacks width for the `*`/`>`/`#` prefix used on desktop, so we
    // use bare 2-letter labels and rely on color for the brand cue.
    let (agent_label, agent_color) = match session.agent_cli {
        "claude" => ("CC", Color::Rgb(217, 119, 87)),    // #D97757 terracotta
        "codex" => ("CD", Color::Rgb(122, 157, 255)),    // #7A9DFF periwinkle
        "opencode" => ("OC", Color::Rgb(74, 222, 128)),  // #4ADE80 emerald
        other => {
            // Leak-free fallback: borrow a stack buffer.
            let s: String = other.chars().take(2).collect::<String>().to_uppercase();
            // Convert to a 'static str by leaking — but we only call this once per session.
            let leaked: &'static str = Box::leak(s.into_boxed_str());
            (leaked, theme.inactive_fg)
        }
    };

    let status = status_short(&session.status);
    let status_color = match session.status {
        SessionStatus::Thinking => theme.proc_misc,
        SessionStatus::Executing => theme.hi_fg,
        SessionStatus::Waiting => grad_at(grad, 50.0),
        SessionStatus::Unknown => theme.inactive_fg,
        SessionStatus::RateLimited => theme.status_fg,
        SessionStatus::Done => theme.inactive_fg,
    };

    let is_1m = session.context_window >= 1_000_000 || session.model.contains("[1m]");
    let model_short = shorten_model(&session.model, is_1m);
    let ctx_color = grad_at(grad, session.context_percent);

    let project = truncate_str(&session.project_name, PROJECT_TRUNCATE);
    let width = area.width as usize;
    // Layout: ► CC <project>      ●<status>  <pct>% <model>
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(format!("{} ", marker), marker_style));
    spans.push(Span::styled(
        format!("{} ", agent_label),
        Style::default().fg(agent_color).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        project,
        Style::default().fg(if selected {
            theme.selected_fg
        } else {
            theme.title
        }),
    ));
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum::<usize>();
    let right_text = format!("{} {}% {}", status, session.context_percent as i64, model_short);
    let pad = width.saturating_sub(used + right_text.chars().count() + 1);
    if pad > 0 {
        spans.push(Span::raw(" ".repeat(pad)));
    }
    spans.push(Span::styled(
        status,
        Style::default().fg(status_color).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        format!(" {:>3.0}%", session.context_percent),
        Style::default().fg(ctx_color).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        format!(" {}", truncate_str(&model_short, MODEL_TRUNCATE)),
        Style::default().fg(theme.graph_text),
    ));

    let mut line = Line::from(spans);
    if selected {
        line = line.style(
            Style::default()
                .bg(theme.selected_bg)
                .fg(theme.selected_fg),
        );
    }
    f.render_widget(Paragraph::new(line), area);
}

/// Row 2: `  47m · 24 turns · 1.2M tok`
/// Row 2: `  Summary: <truncated summary>`.
/// The summary text comes from `App::session_summary` (which already
/// resolves the summary cache, pending-dots animation, and falls back
/// to `initial_prompt` / `first_assistant_text`). We only truncate the
/// rendered string and color it like the other dim helper rows.
fn draw_session_row2(
    f: &mut Frame,
    app: &App,
    session: &crate::model::AgentSession,
    area: Rect,
    theme: &Theme,
) {
    let summary = app.session_summary(session);
    let prefix = format!("  {}: ", crate::locale::t("col.summary"));
    let prefix_w = prefix.chars().count();
    let content = truncate_str(&summary, SUMMARY_TRUNCATE);
    let width = area.width as usize;
    // Pad to right edge so the summary looks anchored at the start of the
    // row, with the dim helper color extending to the column end.
    let pad = width.saturating_sub(prefix_w + content.chars().count());
    let mut spans: Vec<Span> = vec![
        Span::styled(prefix, Style::default().fg(theme.graph_text)),
        Span::styled(content, Style::default().fg(theme.main_fg)),
    ];
    if pad > 0 {
        spans.push(Span::styled(
            " ".repeat(pad),
            Style::default().fg(theme.graph_text),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Row 3: stats + task collapsed into one right-aligned line.
/// Format: `└─ <task> · 47m · 24 turns · 1.2M tok`, truncated to fit and
/// flushed to the right edge of the row. When the task is empty, render
/// `(idle) · <stats>` instead.
fn draw_session_row3(
    f: &mut Frame,
    session: &crate::model::AgentSession,
    area: Rect,
    theme: &Theme,
) {
    let age_str = session.elapsed_display();
    let turns_str = if session.turn_count == 1 {
        "1 turn".to_string()
    } else {
        format!("{} turns", session.turn_count)
    };
    let stats = format!("{} · {} · {} tok", age_str, turns_str, fmt_tokens(session.total_tokens()));
    let task = session
        .current_tasks
        .last()
        .map(|s| s.as_str())
        .unwrap_or("");
    let body = if task.is_empty() {
        format!("(idle) · {}", stats)
    } else {
        format!("{} · {}", truncate_str(task, TASK_TRUNCATE), stats)
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("└─ {body}"),
            Style::default().fg(theme.graph_text),
        )))
        .alignment(Alignment::Right),
        area,
    );
}

/// Render the 5-row tokens panel for the currently selected session.
///
/// Layout: a single 5×2 grid.
/// - **Left column** (label + bar + value): Total row has no bar; the
///   other four rows show a native `LineGauge` whose width is
///   `TOKENS_BAR_W` cells (3 wider than the previous version so the bar
///   reads at a glance from arm's length).
/// - **Right column** mirrors the desktop context panel: a "Token Rate"
///   line on top, three rows of a `braille_graph_multirow` of
///   `app.token_rates` in the middle, and a "Total N" line at the bottom.
///
/// Falls back to a placeholder when no session is selected.
fn draw_tokens(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let session = match app.sessions.get(app.selected) {
        Some(s) => s,
        None => {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " (no session) ",
                    Style::default().fg(theme.inactive_fg),
                ))),
                area,
            );
            return;
        }
    };
    let total_in = session.total_input_tokens;
    let total_out = session.total_output_tokens;
    let total_cache_r = session.total_cache_read;
    let total_cache_w = session.total_cache_create;
    let total_all = total_in + total_out + total_cache_r + total_cache_w;

    // Per-row ratio is the metric's share of total
    let ratio = |v: u64| if total_all > 0 { v as f64 / total_all as f64 } else { 0.0 };

    let label_style = Style::default().fg(theme.title);
    let total_style = Style::default()
        .fg(theme.title)
        .add_modifier(Modifier::BOLD);
    let main_style = Style::default().fg(theme.main_fg);
    let cache_r_style = Style::default().fg(theme.session_id);
    let cache_w_style = Style::default().fg(theme.proc_misc);
    let graph_text = theme.graph_text;

    // Split the panel vertically into 5 rows, then each row into a left
    // (bar) column and a right (graph) column.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    // Right column needs at least 1 char gap from the bar; left column
    // holds 6-char label + 1 gap + bar + 1 gap + value.
    let labels = ["Total", "In", "Out", "CacheR", "CacheW"];
    let values = [total_all, total_in, total_out, total_cache_r, total_cache_w];
    let value_styles = [total_style, main_style, main_style, cache_r_style, cache_w_style];
    let value_colors = [
        theme.title,
        theme.main_fg,
        theme.main_fg,
        theme.session_id,
        theme.proc_misc,
    ];
    let value_strs: Vec<String> = values.iter().map(|v| fmt_tokens(*v)).collect();

    for i in 0..5 {
        let row = rows[i];
        // 6 label + 1 gap + TOKENS_BAR_W bar + 1 gap + value + 1 gap = 20
        // for 46-col iPhone. Right column fills the remainder.
        let total_left = 6 + 1 + TOKENS_BAR_W as usize + 1 + 6 + 1;
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(total_left as u16),
                Constraint::Min(0),
            ])
            .split(row);
        let left = cols[0];
        let right = cols[1];

        // ── left column ────────────────────────────────────────────────
        if i == 0 {
            // Total: label + value, NO bar.
            let lchunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(6),    // label
                    Constraint::Length(1),    // gap
                    Constraint::Min(0),       // value
                ])
                .split(left);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("{:<6}", labels[i]),
                    label_style,
                ))),
                lchunks[0],
            );
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    value_strs[i].clone(),
                    value_styles[i],
                ))),
                lchunks[2],
            );
        } else {
            // Other rows: label + bar + value.
            let lchunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(6),                    // label
                    Constraint::Length(1),                    // gap
                    Constraint::Length(TOKENS_BAR_W),         // bar
                    Constraint::Length(1),                    // gap
                    Constraint::Min(0),                       // value
                ])
                .split(left);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("{:<6}", labels[i]),
                    label_style,
                ))),
                lchunks[0],
            );
            let gauge = LineGauge::default()
                .ratio(ratio(values[i]))
                .filled_style(Style::default().fg(value_colors[i]))
                .unfilled_style(Style::default().fg(theme.meter_bg))
                .label("");
            f.render_widget(gauge, lchunks[2]);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    value_strs[i].clone(),
                    value_styles[i],
                ))),
                lchunks[4],
            );
        }

        // ── right column ───────────────────────────────────────────────
        if i == 0 {
            // Top: "Token Rate" label + current rate per minute.
            let rates: Vec<f64> = app.token_rates.iter().copied().collect();
            let max_rate = rates.iter().copied().fold(1.0_f64, f64::max);
            let current_rate = *rates.last().unwrap_or(&0.0);
            let pct = if max_rate > 0.0 {
                (current_rate / max_rate * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            };
            let cpu_grad = make_gradient(
                theme.cpu_grad.start,
                theme.cpu_grad.mid,
                theme.cpu_grad.end,
            );
            let rate_color = grad_at(&cpu_grad, pct);
            let rate_label = crate::locale::t("context.token_rate");
            let rchunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(rate_label.chars().count() as u16 + 1),
                    Constraint::Min(0),
                ])
                .split(right);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    rate_label,
                    Style::default().fg(graph_text),
                ))),
                rchunks[0],
            );
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("{}/min", fmt_tokens(current_rate as u64)),
                    Style::default().fg(rate_color),
                ))),
                rchunks[1],
            );
        } else if (1..=3).contains(&i) {
            // Middle 3 rows: braille graph of token rate history.
            let rates: Vec<f64> = app.token_rates.iter().copied().collect();
            let max_rate = rates.iter().copied().fold(1.0_f64, f64::max);
            let normalized: Vec<f64> = rates.iter().map(|&v| v / max_rate).collect();
            let cpu_grad = make_gradient(
                theme.cpu_grad.start,
                theme.cpu_grad.mid,
                theme.cpu_grad.end,
            );
            let graph_w = right.width.saturating_sub(1) as usize;
            // Render the 3-row graph once and pick the row matching `i`.
            // (Cheaper than re-running braille_graph_multirow per row, and
            // keeps the data pipeline in a single place.)
            let rows_spans = braille_graph_multirow(
                &normalized,
                graph_w,
                3,
                &cpu_grad,
                graph_text,
            );
            let row_spans = rows_spans
                .get(i - 1)
                .cloned()
                .unwrap_or_default();
            f.render_widget(Paragraph::new(Line::from(row_spans)), right);
        } else {
            // Bottom (i == 4): "Total <value>".
            let total_label = crate::locale::t("context.total");
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(total_label, Style::default().fg(graph_text)),
                    Span::styled(
                        format!(" {}", fmt_tokens(total_all)),
                        Style::default().fg(theme.main_fg),
                    ),
                ])),
                right,
            );
        }
    }
}

/// Chat tail: up to 5 recent user/assistant messages from the selected session.
/// Falls back to a placeholder when there are no messages.
fn draw_chat(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let Some(session) = app.sessions.get(app.selected) else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " (no session) ",
                Style::default().fg(theme.inactive_fg),
            ))),
            area,
        );
        return;
    };

    if session.chat_messages.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " no chat yet ",
                Style::default().fg(theme.inactive_fg),
            ))),
            area,
        );
        return;
    }

    let h = area.height as usize;
    // Fill the chat panel with as many recent messages as fit, up to the
    // actual panel height. CHAT_VISIBLE is no longer a hard cap — the only
    // ceiling is the rows the layout gives us. Older messages scroll off.
    let take = session.chat_messages.len().min(h);
    let start = session.chat_messages.len() - take;
    let mut lines: Vec<Line> = session.chat_messages[start..]
        .iter()
        .map(|m| chat_line(m, theme))
        .collect();
    // Bottom-pin: when the chat panel is taller than the message tail (because
    // sessions < 7 left extra rows), pad with empty lines above so the most
    // recent message sits at the bottom of the panel.
    let pad = h.saturating_sub(take);
    for _ in 0..pad {
        lines.insert(0, Line::from(""));
    }
    f.render_widget(Paragraph::new(lines), area);
}

/// Format a single chat line with `U ` or `A ` prefix.
fn chat_line(msg: &ChatMessage, theme: &Theme) -> Line<'static> {
    let (prefix, color) = match msg.role {
        ChatRole::User => ("U", theme.hi_fg),
        // proc_misc (green across themes) signals "active/in-progress" — the
        // assistant role is the agent's own voice, distinct from the user's hi_fg.
        ChatRole::Assistant => ("A", theme.proc_misc),
    };
    Line::from(vec![
        Span::styled(
            format!("{} ", prefix),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_str(&msg.text, TASK_TRUNCATE),
            Style::default().fg(theme.main_fg),
        ),
    ])
}

/// Compact footer: ↑↓ sel ↵ jump / filter x kill ? help q quit
fn draw_footer(f: &mut Frame, area: Rect, theme: &Theme) {
    let parts: &[(&str, &str)] = &[
        ("↑↓", "sel"),
        ("↵", "jump"),
        ("/", "filter"),
        ("x", "kill"),
        ("?", "help"),
        ("q", "quit"),
    ];
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(" "));
    for (i, (key, label)) in parts.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(*key, Style::default().fg(theme.hi_fg)));
        spans.push(Span::styled(
            format!(" {}", label),
            Style::default().fg(theme.main_fg),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Left),
        area,
    );
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
            text.contains("abtop v"),
            "full layout should render title\n{text}"
        );
    }

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

    #[test]
    fn iphone_mode_quota_section_present() {
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        // populate_demo overwrites rate_limits — push mmx after it so the
        // buckets have both a percentage (for the gauge label) and a reset
        // timestamp (so the "in X" text appears).
        demo::populate_demo(&mut app);
        let now = chrono::Utc::now().timestamp() as u64;
        app.rate_limits.push(crate::model::RateLimitInfo {
            source: "mmx".to_string(),
            five_hour_pct: Some(42.0),
            five_hour_resets_at: Some(now + 2 * 3600 + 13 * 60),
            seven_day_pct: Some(15.0),
            seven_day_resets_at: Some(now + 6 * 86400 + 12 * 3600),
            updated_at: None,
        });
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        assert!(
            text.contains("mmx · quota"),
            "mmx · quota divider\n{text}"
        );
        assert!(text.contains("5h"), "5h bucket\n{text}");
        assert!(text.contains("7d"), "7d bucket\n{text}");
        // Quota gauges still show the usage percentage on the bar label.
        assert!(text.contains("%"), "percentage should appear on quota bar\n{text}");
        // Reset text uses the "in" prefix from format_reset_time.
        assert!(
            text.contains("in "),
            "reset text should carry the 'in ' prefix\n{text}"
        );
    }

    #[test]
    fn iphone_mode_quota_uses_native_gauge() {
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        // Add a rate limit so the gauge has data to render
        app.rate_limits.push(crate::model::RateLimitInfo {
            source: "mmx".to_string(),
            five_hour_pct: Some(50.0),
            five_hour_resets_at: Some(chrono::Utc::now().timestamp() as u64 + 7200),
            seven_day_pct: Some(30.0),
            seven_day_resets_at: Some(chrono::Utc::now().timestamp() as u64 + 172800),
            updated_at: None,
        });
        demo::populate_demo(&mut app);
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        // Native LineGauge renders with horizontal line characters (default ─)
        // for both the filled and unfilled portions, colored by their styles.
        // We just need to confirm the gauge is actually being rendered into the
        // quota rows — assert that "5h" row has a run of ─ characters where
        // the manual █/░ bar used to live.
        let line_5h = text
            .lines()
            .find(|l| l.contains("5h"))
            .expect("5h row should render");
        assert!(
            line_5h.contains('─'),
            "native LineGauge should render horizontal-line chars in the 5h row\n{line_5h}\n--- full ---\n{text}"
        );
    }

    #[test]
    fn iphone_mode_quota_bucket_labels_plain_text() {
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        demo::populate_demo(&mut app);
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        // The 5h / 7d bucket labels render as bare plain text (no icon prefix
        // such as ↻ or ⤴). The reset countdown is the only text adjacent to
        // the label, so we just confirm the bare labels are present.
        assert!(text.contains("5h"), "5h label\n{text}");
        assert!(text.contains("7d"), "7d label\n{text}");
    }

    #[test]
    fn iphone_mode_quota_no_duplicate_in_prefix() {
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        demo::populate_demo(&mut app);
        // populate_demo overwrites rate_limits — push mmx after it.
        let now = chrono::Utc::now().timestamp() as u64;
        app.rate_limits.push(crate::model::RateLimitInfo {
            source: "mmx".to_string(),
            five_hour_pct: Some(50.0),
            five_hour_resets_at: Some(now + 2 * 3600 + 13 * 60), // 2h 13m
            seven_day_pct: Some(30.0),
            seven_day_resets_at: Some(now + 13 * 86400 + 4 * 3600), // 13d 4h
            updated_at: None,
        });
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        assert!(
            !text.contains("in in"),
            "should not duplicate 'in' prefix\n{text}"
        );
        assert!(
            text.contains("2h 13m"),
            "5h reset time should render\n{text}"
        );
        assert!(
            text.contains("13d 4h"),
            "7d reset time should render\n{text}"
        );
    }

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
        // Status is now plain text — no icon prefix.
        assert!(
            text.contains("Think")
                || text.contains("Exec")
                || text.contains("Wait")
                || text.contains("Rate")
                || text.contains("Done")
                || text.contains("Unk"),
            "status label\n{text}"
        );
        assert!(text.contains("turns"), "stats row\n{text}");
        assert!(text.contains("└─"), "task row\n{text}");
    }

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
        assert!(text.contains("mmx · quota"), "mmx · quota divider\n{text}");
        assert!(text.contains(" sessions "), "sessions divider\n{text}");
        assert!(text.contains(" tokens "), "tokens divider\n{text}");
        assert!(text.contains("chats"), "chat divider\n{text}");
    }

    #[test]
    fn iphone_mode_tokens_panel_present() {
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        demo::populate_demo(&mut app);
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        assert!(text.contains(" tokens "), "tokens divider\n{text}");
        assert!(text.contains("Total"), "Total label\n{text}");
        assert!(text.contains("In "), "In label\n{text}");
        assert!(text.contains("Out"), "Out label\n{text}");
        assert!(text.contains("CacheR"), "CacheR label\n{text}");
        assert!(text.contains("CacheW"), "CacheW label\n{text}");
    }

    /// Helper: build a minimal AgentSession with the given token counts and
    /// a project name so session rows are distinguishable in the buffer.
    fn make_test_session(
        name: &str,
        input: u64,
        output: u64,
        cache_r: u64,
        cache_w: u64,
    ) -> crate::model::AgentSession {
        crate::model::AgentSession {
            agent_cli: "claude",
            pid: 0,
            session_id: String::new(),
            cwd: "/tmp".into(),
            project_name: name.into(),
            started_at: 0,
            status: crate::model::SessionStatus::Waiting,
            model: "claude-sonnet-4-6".into(),
            effort: String::new(),
            context_percent: 0.0,
            total_input_tokens: input,
            total_output_tokens: output,
            total_cache_read: cache_r,
            total_cache_create: cache_w,
            turn_count: 1,
            current_tasks: vec![],
            mem_mb: 0,
            version: String::new(),
            git_branch: String::new(),
            git_added: 0,
            git_modified: 0,
            token_history: Vec::new(),
            context_history: Vec::new(),
            compaction_count: 0,
            context_window: 0,
            subagents: Vec::new(),
            mem_file_count: 0,
            mem_line_count: 0,
            children: Vec::new(),
            initial_prompt: String::new(),
            first_assistant_text: String::new(),
            chat_messages: Vec::new(),
            tool_calls: Vec::new(),
            pending_since_ms: 0,
            thinking_since_ms: 0,
            file_accesses: Vec::new(),
            config_root: String::new(),
        }
    }

    #[test]
    fn iphone_mode_tokens_use_selected_session_only() {
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        // Two sessions with very different token counts.
        let s0 = make_test_session("p0", 100, 200, 300, 400); // total = 1000
        let s1 = make_test_session("p1", 10_000_000, 0, 0, 0); // total = 10M
        app.sessions.push(s0);
        app.sessions.push(s1);
        app.selected = 0; // select the first session (small tokens)

        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        assert!(
            text.contains("1.0k") || text.contains("1000") || text.contains("1k"),
            "selected session's small token total should appear\n{text}"
        );
        // The second session's 10M tokens DO appear in its row 2 ("10M tok"),
        // so we can't simply assert !contains("10M"). Instead, scope the
        // assertion to the tokens panel — between the " tokens " divider and
        // the chat divider. In that slice, the only token count visible
        // should be the selected session's totals (1k / 100 / 200 / 300 / 400).
        let tokens_panel_start = text.find(" tokens ").expect("tokens divider");
        let tokens_panel_end = text[tokens_panel_start..]
            .find("chats")
            .map(|i| tokens_panel_start + i)
            .expect("chat divider after tokens panel");
        let tokens_panel = &text[tokens_panel_start..tokens_panel_end];
        assert!(
            !tokens_panel.contains("10M") && !tokens_panel.contains("10.0M"),
            "second session's 10M tokens must NOT appear in the tokens panel slice:\n{tokens_panel}\n--- full ---\n{text}"
        );
        assert!(
            tokens_panel.contains("1k") || tokens_panel.contains("1000") || tokens_panel.contains("1.0k"),
            "selected session's 1k total must appear in the tokens panel slice:\n{tokens_panel}\n--- full ---\n{text}"
        );
    }

    #[test]
    fn iphone_mode_tokens_gauge_has_no_percentage() {
        // Tokens gauges must NOT show a percentage label (Total has no gauge
        // at all; other rows use a bare LineGauge). The quota panel still uses
        // "%" on its bars, so we scope this check to the tokens slice only.
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        let mut s = make_test_session("p0", 1000, 2000, 3000, 4000);
        s.project_name = "p0".into();
        app.sessions.push(s);
        // Add a quota row so the tokens slice is well-bounded.
        let now = chrono::Utc::now().timestamp() as u64;
        app.rate_limits.push(crate::model::RateLimitInfo {
            source: "mmx".to_string(),
            five_hour_pct: Some(50.0),
            five_hour_resets_at: Some(now + 2 * 3600 + 13 * 60),
            seven_day_pct: Some(30.0),
            seven_day_resets_at: Some(now + 13 * 86400 + 4 * 3600),
            updated_at: None,
        });
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        let tokens_start = text.find("tokens").expect("tokens divider");
        let chats_start = text.find("chats").expect("chats divider");
        let tokens_slice = &text[tokens_start..chats_start];
        assert!(
            !tokens_slice.contains('%'),
            "tokens panel should not show percentages on its bars\nslice: {tokens_slice}"
        );
    }

    #[test]
    fn iphone_mode_no_sessions_does_not_panic() {
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        // do NOT populate demo
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        assert!(text.contains("abtop v"), "meta still renders\n{text}");
        assert!(text.contains("q quit"), "footer still renders\n{text}");
    }

    #[test]
    fn iphone_mode_caps_at_5_sessions() {
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        for i in 0..10 {
            app.sessions.push(crate::model::AgentSession {
                agent_cli: "claude",
                pid: 1000 + i as u32,
                session_id: format!("s{i}"),
                cwd: "/tmp".into(),
                project_name: format!("p{i:02}"),
                started_at: 0,
                status: crate::model::SessionStatus::Waiting,
                model: "claude-sonnet-4-6".into(),
                effort: String::new(),
                context_percent: 10.0 + i as f64,
                total_input_tokens: 0,
                total_output_tokens: 0,
                total_cache_read: 0,
                total_cache_create: 0,
                turn_count: 1,
                current_tasks: vec![],
                mem_mb: 0,
                version: String::new(),
                git_branch: String::new(),
                git_added: 0,
                git_modified: 0,
                token_history: Vec::new(),
                context_history: Vec::new(),
                compaction_count: 0,
                context_window: 0,
                subagents: Vec::new(),
                mem_file_count: 0,
                mem_line_count: 0,
                children: Vec::new(),
                initial_prompt: String::new(),
                first_assistant_text: String::new(),
                chat_messages: Vec::new(),
                tool_calls: Vec::new(),
                pending_since_ms: 0,
                thinking_since_ms: 0,
                file_accesses: Vec::new(),
                config_root: String::new(),
            });
        }
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        assert!(text.contains("p00"), "first session should render\n{text}");
        assert!(text.contains("p04"), "5th session should render\n{text}");
        assert!(!text.contains("p05"), "6th session should NOT render\n{text}");
        assert!(!text.contains("p09"), "10th session should NOT render\n{text}");
    }

    /// Helper: find the 1-indexed line in the rendered buffer that contains `needle`.
    /// Returns 0 if not found.
    fn line_of(text: &str, needle: &str) -> usize {
        for (i, line) in text.lines().enumerate() {
            if line.contains(needle) {
                return i + 1;
            }
        }
        0
    }

    /// Helper: find the last 1-indexed line containing `needle`. Returns 0 if not found.
    fn last_line_of(text: &str, needle: &str) -> usize {
        text.lines()
            .enumerate()
            .filter(|(_, l)| l.contains(needle))
            .last()
            .map(|(i, _)| i + 1)
            .unwrap_or(0)
    }

    #[test]
    fn iphone_mode_chat_expands_when_few_sessions() {
        // Same dimensions (46x35), two scenarios: 5 sessions vs 2 sessions.
        // With 5 sessions (the cap): chat = whatever rows remain after the
        // fixed rows (meta/quotas/tokens/dividers/footer).
        // With 2 sessions: chat should expand to absorb the leftover rows.
        //
        // The 5-session cap means 2 sessions use 6 rows instead of 15,
        // leaving 9 extra rows for chat -> chat divider should be ~9 rows higher.
        let make_app = |n: usize, with_chat: bool| {
            let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
            for i in 0..n {
                let mut s = crate::model::AgentSession {
                    agent_cli: "claude",
                    pid: 1000 + i as u32,
                    session_id: format!("s{i}"),
                    cwd: "/tmp".into(),
                    project_name: format!("p{i}"),
                    started_at: 0,
                    status: crate::model::SessionStatus::Waiting,
                    model: "claude-sonnet-4-6".into(),
                    effort: String::new(),
                    context_percent: 10.0,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    total_cache_read: 0,
                    total_cache_create: 0,
                    turn_count: 1,
                    current_tasks: vec![],
                    mem_mb: 0,
                    version: String::new(),
                    git_branch: String::new(),
                    git_added: 0,
                    git_modified: 0,
                    token_history: Vec::new(),
                    context_history: Vec::new(),
                    compaction_count: 0,
                    context_window: 0,
                    subagents: Vec::new(),
                    mem_file_count: 0,
                    mem_line_count: 0,
                    children: Vec::new(),
                    initial_prompt: String::new(),
                    first_assistant_text: String::new(),
                    chat_messages: Vec::new(),
                    tool_calls: Vec::new(),
                    pending_since_ms: 0,
                    thinking_since_ms: 0,
                    file_accesses: Vec::new(),
                    config_root: String::new(),
                };
                if with_chat {
                    s.chat_messages.push(crate::model::ChatMessage {
                        role: crate::model::ChatRole::User,
                        text: "hi".into(),
                    });
                }
                app.sessions.push(s);
            }
            app
        };

        // 5 sessions baseline (at the cap).
        let app5 = make_app(5, true);
        let backend5 = TestBackend::new(46, 35);
        let mut term5 = Terminal::new(backend5).unwrap();
        term5
            .draw(|f| draw_iphone_mode(f, &app5, f.area(), &app5.theme))
            .unwrap();
        let text5 = format!("{}", term5.backend());
        let chat_div_5 = line_of(&text5, "chats");
        assert!(chat_div_5 > 0, "chat divider should render\n{text5}");

        // 2 sessions: chat divider should appear noticeably earlier (higher up).
        let app2 = make_app(2, true);
        let backend2 = TestBackend::new(46, 35);
        let mut term2 = Terminal::new(backend2).unwrap();
        term2
            .draw(|f| draw_iphone_mode(f, &app2, f.area(), &app2.theme))
            .unwrap();
        let text2 = format!("{}", term2.backend());
        let chat_div_2 = line_of(&text2, "chats");
        assert!(chat_div_2 > 0, "chat divider should render\n{text2}");

        // 3 fewer sessions * 3 rows = 9 fewer session rows -> chat divider
        // should move up by ~9 rows.
        assert!(
            chat_div_5.saturating_sub(chat_div_2) >= 6,
            "chat panel should expand when sessions < 5: \
             chat_div_5={chat_div_5}, chat_div_2={chat_div_2}\n--- 5 sessions ---\n{text5}\n--- 2 sessions ---\n{text2}"
        );
    }

    #[test]
    fn iphone_mode_chat_messages_bottom_pinned() {
        // With 1 message and an expanded chat area, the message should appear
        // at the bottom of the chat panel, not the top.
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        app.sessions.push(crate::model::AgentSession {
            agent_cli: "claude",
            pid: 1,
            session_id: "solo".into(),
            cwd: "/tmp".into(),
            project_name: "solo".into(),
            started_at: 0,
            status: crate::model::SessionStatus::Waiting,
            model: "claude-sonnet-4-6".into(),
            effort: String::new(),
            context_percent: 0.0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_read: 0,
            total_cache_create: 0,
            turn_count: 1,
            current_tasks: vec![],
            mem_mb: 0,
            version: String::new(),
            git_branch: String::new(),
            git_added: 0,
            git_modified: 0,
            token_history: Vec::new(),
            context_history: Vec::new(),
            compaction_count: 0,
            context_window: 0,
            subagents: Vec::new(),
            mem_file_count: 0,
            mem_line_count: 0,
            children: Vec::new(),
            initial_prompt: String::new(),
            first_assistant_text: String::new(),
            chat_messages: vec![crate::model::ChatMessage {
                role: crate::model::ChatRole::User,
                text: "BOTTOM_ANCHOR".into(),
            }],
            tool_calls: Vec::new(),
            pending_since_ms: 0,
            thinking_since_ms: 0,
            file_accesses: Vec::new(),
            config_root: String::new(),
        });
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        let chat_div = line_of(&text, "chats");
        let footer_div = line_of(&text, "────");
        let anchor = line_of(&text, "BOTTOM_ANCHOR");
        assert!(chat_div > 0, "chat divider must render\n{text}");
        assert!(anchor > chat_div, "message below chat divider\n{text}");
        // Footer divider is the second occurrence of `────` (first is between
        // quota and sessions). Find last one to locate chat panel end.
        let last_dashes = last_line_of(&text, "────");
        assert!(last_dashes > 0, "footer divider must render\n{text}");
        // With 1 session, chat panel is large; the single message should sit
        // closer to the footer divider than to the chat divider (bottom-pinned).
        let chat_h = last_dashes - chat_div - 1;
        let dist_from_top = anchor - chat_div - 1;
        let dist_from_bottom = chat_h - dist_from_top;
        assert!(
            dist_from_bottom < dist_from_top,
            "message should be bottom-pinned: \
             chat_h={chat_h}, dist_from_top={dist_from_top}, dist_from_bottom={dist_from_bottom}\n{text}"
        );
    }

    #[test]
    fn iphone_mode_chat_fills_panel_beyond_five() {
        // The chat panel is no longer capped at 5 messages — it should fill
        // the available rows. With 1 session there is plenty of vertical
        // room, so 12 messages should all appear in the rendered buffer.
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        let mut s = make_test_session("p0", 1000, 2000, 3000, 4000);
        for i in 0..12 {
            let role = if i % 2 == 0 {
                crate::model::ChatRole::User
            } else {
                crate::model::ChatRole::Assistant
            };
            s.chat_messages.push(crate::model::ChatMessage {
                role,
                text: format!("msg{i:02}"),
            });
        }
        app.sessions.push(s);

        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        for i in 0..12 {
            let needle = format!("msg{i:02}");
            assert!(
                text.contains(&needle),
                "chat message {needle} should render (chat panel fills all available rows)\n{text}"
            );
        }
    }
}
