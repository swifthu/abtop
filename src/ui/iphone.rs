use crate::app::App;
use crate::model::{ChatMessage, ChatRole, SessionStatus};
use crate::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::quota::format_reset_time;
use super::sessions::shorten_model;
use super::{fmt_tokens, grad_at, make_gradient, truncate_str};

/// Maximum number of sessions rendered in the iPhone-mode list.
const MAX_VISIBLE_SESSIONS: usize = 5;
/// Rows of chat tail shown for the selected session.
const CHAT_VISIBLE: usize = 5;
/// Max chars for the rendered task text (`└─ ...`).
const TASK_TRUNCATE: usize = 38;
/// Max chars for the project column.
const PROJECT_TRUNCATE: usize = 12;
/// Max chars for the model name shown in the session row 1.
const MODEL_TRUNCATE: usize = 12;

/// Source IDs rendered in the quota section, in display order.
const QUOTA_SOURCES: &[&str] = &["mmx", "claude"];

/// Display label for a quota source ID.
/// "mmx" stays as the CLI/internal ID; "claude" is abbreviated to "cl".
fn quota_label(source: &str) -> &'static str {
    match source {
        "mmx" => "mmx",
        "claude" => "cl ",
        _ => "??",
    }
}

/// Short plain-text status label (e.g. "Think", "Wait"). No icon prefix —
/// visual signal comes from color + the context row 3 task line.
pub(crate) fn status_short(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Thinking => "Think",
        SessionStatus::Executing => "Exec",
        SessionStatus::Waiting => "Wait",
        SessionStatus::Unknown => "Unk",
        SessionStatus::RateLimited => "Rate",
        SessionStatus::Done => "Done",
    }
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
    let chat_h = if app.sessions.is_empty() {
        1u16
    } else {
        CHAT_VISIBLE as u16
    };
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
    let actual_chat_h = if app.sessions.is_empty() { 1 } else { chat_h as usize };

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
    draw_divider(f, chunks[1], theme, "quota", theme.cpu_box);
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

/// Render two quota rows: one per source (mmx, claude), each showing
/// both buckets inline (e.g. `mmx 5h 65% ↻2h  7d 88% ↻3d`).
fn draw_quota(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let mut lines: Vec<Line> = Vec::new();
    for source in QUOTA_SOURCES.iter() {
        let rl = app
            .rate_limits
            .iter()
            .find(|r| r.source.eq_ignore_ascii_case(source));
        let label = quota_label(source);
        let mut spans: Vec<Span> = vec![Span::styled(
            format!("{} ", label),
            Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD),
        )];
        match rl {
            Some(rl) => {
                for (bucket_label, pct, reset) in [
                    ("5h", &rl.five_hour_pct, &rl.five_hour_resets_at),
                    ("7d", &rl.seven_day_pct, &rl.seven_day_resets_at),
                ] {
                    let pct_str = match pct {
                        Some(p) => format!("{:>3.0}%", p),
                        None => "  —  ".to_string(),
                    };
                    let reset_str = match (reset, pct) {
                        (Some(ts), Some(_)) => format!("↻{}", format_reset_time(*ts)),
                        _ => String::new(),
                    };
                    let color = match pct {
                        Some(p) if *p >= 80.0 => theme.status_fg,
                        Some(p) if *p >= 60.0 => theme.warning_fg,
                        Some(_) => theme.proc_misc,
                        None => theme.inactive_fg,
                    };
                    spans.push(Span::styled(
                        format!(" {bucket_label} "),
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
                spans.push(Span::styled(
                    " —  — ",
                    Style::default().fg(theme.inactive_fg),
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
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
        draw_session_row2(f, session, row_block[1], theme);
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
        SessionStatus::Thinking | SessionStatus::Executing => theme.proc_misc,
        SessionStatus::Waiting => theme.graph_text,
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
        status.to_string(),
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
fn draw_session_row2(
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
    let text = format!("  {} · {} · {} tok", age_str, turns_str, fmt_tokens(session.total_tokens()));
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(theme.graph_text),
        ))),
        area,
    );
}

/// Row 3: `  └─ Edit src/pay.rs`
fn draw_session_row3(
    f: &mut Frame,
    session: &crate::model::AgentSession,
    area: Rect,
    theme: &Theme,
) {
    let task = session
        .current_tasks
        .last()
        .map(|s| s.as_str())
        .unwrap_or("");
    let body = if task.is_empty() {
        "(idle)".to_string()
    } else {
        format!("└─ {}", truncate_str(task, TASK_TRUNCATE))
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("  {}", body),
            Style::default().fg(theme.graph_text),
        ))),
        area,
    );
}

/// Render the 5-row aggregated tokens panel (Total / In / Out / CacheR / CacheW).
/// Sums each bucket across all sessions for an at-a-glance view of context use.
fn draw_tokens(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let total_in: u64 = app.sessions.iter().map(|s| s.total_input_tokens).sum();
    let total_out: u64 = app.sessions.iter().map(|s| s.total_output_tokens).sum();
    let total_cache_r: u64 = app.sessions.iter().map(|s| s.total_cache_read).sum();
    let total_cache_w: u64 = app.sessions.iter().map(|s| s.total_cache_create).sum();
    let total_all = total_in + total_out + total_cache_r + total_cache_w;

    let label_style = Style::default().fg(theme.graph_text);
    let total_style = Style::default().fg(theme.title).add_modifier(Modifier::BOLD);
    let cache_w_style = Style::default().fg(theme.proc_misc);
    let cache_r_style = Style::default().fg(theme.session_id);
    let main_style = Style::default().fg(theme.main_fg);

    let label_w = "CacheR".chars().count(); // 6, the longest label
    let lines = vec![
        Line::from(vec![
            Span::styled(format!(" {:<w$}", "Total", w = label_w), label_style),
            Span::styled(" ", Style::default()),
            Span::styled(fmt_tokens(total_all), total_style),
        ]),
        Line::from(vec![
            Span::styled(format!(" {:<w$}", "In", w = label_w), label_style),
            Span::styled(" ", Style::default()),
            Span::styled(fmt_tokens(total_in), main_style),
        ]),
        Line::from(vec![
            Span::styled(format!(" {:<w$}", "Out", w = label_w), label_style),
            Span::styled(" ", Style::default()),
            Span::styled(fmt_tokens(total_out), main_style),
        ]),
        Line::from(vec![
            Span::styled(format!(" {:<w$}", "CacheR", w = label_w), label_style),
            Span::styled(" ", Style::default()),
            Span::styled(fmt_tokens(total_cache_r), cache_r_style),
        ]),
        Line::from(vec![
            Span::styled(format!(" {:<w$}", "CacheW", w = label_w), label_style),
            Span::styled(" ", Style::default()),
            Span::styled(fmt_tokens(total_cache_w), cache_w_style),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), area);
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
    // Cap rendered messages at CHAT_VISIBLE — anything older scrolled off.
    let take = session.chat_messages.len().min(CHAT_VISIBLE);
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
        demo::populate_demo(&mut app);
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        assert!(
            text.contains("mmx  —") || text.contains("mmx 5h") || text.contains("mmx  5h"),
            "mmx quota row\n{text}"
        );
        assert!(text.contains("cl   5h"), "claude quota row\n{text}");
        assert!(text.contains("5h"), "5h bucket\n{text}");
        assert!(text.contains("7d"), "7d bucket\n{text}");
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
        assert!(text.contains(" quota "), "quota divider\n{text}");
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
        // With 5 sessions (the cap): chat = CHAT_VISIBLE (5) rows.
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
}
