//! The render layer (D3): one pure `ui(frame, &state)` — no state changes,
//! testable against ratatui's `TestBackend` headlessly.
//!
//! Layout (an 80x24+ terminal):
//!
//! ```text
//! + OEE bench — line 1 [broker, updated Ns ago, messages] -------+
//! | OEE (shift)  [====58.6%====]  run normal-42                  |
//! | A  [==]  P  [====]  Q  [===]   (three small gauges)          |
//! | Parts: 126   status: run   verdicts: good good cracked ...   |
//! | OEE by minute: ▂▄▆▂▄  (sparkline)                            |
//! | footer: q quit | broker ... | parse errors N | stream ended  |
//! +---------------------------------------------------------------+
//! ```

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Sparkline};
use ratatui::Frame;

use crate::state::{zone, DashboardState, Zone};

/// Renders the whole dashboard. Panics nowhere: a gauge ratio is clamped
/// before use (Gauge panics outside 0..=1 — corrupt payloads must not kill
/// the display).
pub fn ui(f: &mut Frame, state: &DashboardState, now: std::time::Instant) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .areas(f.area());

    render_header(f, header, state, now);
    render_body(f, body, state);
    render_footer(f, footer, state);
}

fn render_header(f: &mut Frame, area: Rect, state: &DashboardState, now: std::time::Instant) {
    let updated = match state.last_update {
        Some(at) => format!("updated {} s ago", now.duration_since(at).as_secs()),
        None => "waiting for data".to_string(),
    };
    let connection = if state.connected {
        Span::styled(
            format!("● {}  ", state.broker_addr),
            Style::new().fg(Color::Green),
        )
    } else {
        Span::styled(
            format!("○ {} (reconnecting)  ", state.broker_addr),
            Style::new().fg(Color::Red),
        )
    };
    let line = Line::from(vec![
        Span::styled(" OEE bench — line 1  ", Style::new().bold()),
        connection,
        Span::raw(updated),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_body(f: &mut Frame, area: Rect, state: &DashboardState) {
    let [gauges, strip, spark] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .areas(area);

    // The big OEE gauge + the three component gauges.
    let [oee_area, apq] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(gauges);
    let shift = state.shift;
    render_gauge(
        f,
        oee_area,
        " OEE (shift) ",
        shift.oee,
        Some((shift.t_to_ms / 1000) as u64),
    );
    let [a_area, p_area, q_area] = Layout::horizontal([Constraint::Ratio(1, 3); 3]).areas(apq);
    render_gauge(f, a_area, " A ", shift.a, None);
    render_gauge(f, p_area, " P ", shift.p, None);
    render_gauge(f, q_area, " Q ", shift.q, None);

    // The live strip: counter, machine status, Q verdicts.
    let status = state.a_state.as_deref().unwrap_or("—");
    let verdicts = if state.verdicts.is_empty() {
        "—".to_string()
    } else {
        state
            .verdicts
            .iter()
            .map(|verdict| {
                if verdict == "good" {
                    // dot markers keep the strip narrow across fonts
                    "●"
                } else {
                    "○"
                }
            })
            .collect::<String>()
            + " (latest Q verdicts: ● good, ○ cracked)"
    };
    let run = state.run_id.as_deref().unwrap_or("—");
    let counter = state
        .count
        .map(|c| c.to_string())
        .unwrap_or_else(|| "—".into());
    let strip_text = Line::from(vec![
        Span::styled(" parts ", Style::new().bold()),
        Span::raw(format!("{counter}   ")),
        Span::styled("status", Style::new().bold()),
        Span::raw(format!(" {status}   ")),
        Span::styled("run", Style::new().bold()),
        Span::raw(format!(" {run}   ")),
        Span::styled("ms run/planned", Style::new().bold()),
        Span::raw(format!(" {}/{}", shift.run_ms, shift.planned_ms)),
    ]);
    let verdict_line = Line::from(Span::raw(verdicts));
    f.render_widget(
        Paragraph::new(vec![strip_text, verdict_line]).block(
            Block::new()
                .borders(Borders::TOP)
                .border_style(Style::new().fg(Color::DarkGray)),
        ),
        strip,
    );

    // The minute-window OEE history.
    let sparkline = Sparkline::default()
        .block(
            Block::new()
                .title(" OEE by minute (per mille) ")
                .borders(Borders::NONE),
        )
        .data(&state.history)
        .max(1000);
    f.render_widget(sparkline, spark);
}

fn render_gauge(f: &mut Frame, area: Rect, title: &str, value: f32, at_s: Option<u64>) {
    // Corrupt payloads must not panic the render: clamp before Gauge sees it.
    let ratio = if value.is_finite() {
        value.clamp(0.0, 1.0) as f64
    } else {
        0.0
    };
    let suffix = at_s
        .map(|seconds| format!(" @ {seconds}s"))
        .unwrap_or_default();
    let gauge = Gauge::default()
        .block(Block::new().title(format!("{title}{suffix}")))
        .ratio(ratio)
        .label(format!("{:.1}%", ratio * 100.0))
        .gauge_style(match zone(value) {
            Zone::Green => Style::new().fg(Color::Green).bg(Color::Black),
            Zone::Yellow => Style::new().fg(Color::Yellow).bg(Color::Black),
            Zone::Red => Style::new().fg(Color::Red).bg(Color::Black),
        });
    f.render_widget(gauge, area);
}

fn render_footer(f: &mut Frame, area: Rect, state: &DashboardState) {
    let mut spans = vec![
        Span::styled(" q: quit ", Style::new().add_modifier(Modifier::REVERSED)),
        Span::raw("  "),
        Span::styled(
            format!("{} messages", state.messages),
            Style::new().fg(Color::Gray),
        ),
        Span::raw("  "),
    ];
    if state.parse_errors > 0 {
        spans.push(Span::styled(
            format!("{} unparsed payloads", state.parse_errors),
            Style::new().fg(Color::Yellow),
        ));
        spans.push(Span::raw("  "));
    }
    if state.finished {
        spans.push(Span::styled("stream ended", Style::new().fg(Color::Cyan)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::time::Instant;

    /// Renders a fully-populated and an empty state into a TestBackend: no
    /// panics, non-blank content (the "by eye" check, automated).
    #[test]
    fn renders_populated_and_empty_states_without_panicking() {
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        let now = Instant::now();

        let mut state = DashboardState::new("127.0.0.1:1883");
        state.connected = true;
        state.on_message(
            "oee/line1/oee",
            r#"{"scope":"shift","run_id":"normal-42","t_from_ms":0,"t_to_ms":60000,"planned_ms":60000,"run_ms":50400,"parts":126,"good":88,"total":126,"a":0.840,"p":1.000,"q":0.698,"oee":0.586}"#,
            now,
        );
        state.on_message("oee/line1/p/count", r#"{"count":126,"t_ms":59000}"#, now);
        state.on_message("oee/line1/a/status", r#"{"state":"run","t_ms":58000}"#, now);
        state.history = vec![420, 586];
        terminal.draw(|f| ui(f, &state, now)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer
            .content()
            .iter()
            .filter(|cell| cell.symbol() != " ")
            .map(|cell| cell.symbol().to_string())
            .collect();
        assert!(content.contains("OEE"));
        assert!(content.contains("58.6%"));
        assert!(content.contains("126"));
        assert!(content.contains("line1"));

        // An empty, disconnected state renders too (zeros, not NaNs).
        let empty = DashboardState::new("127.0.0.1:1883");
        terminal.draw(|f| ui(f, &empty, now)).unwrap();
    }

    #[test]
    fn clamps_out_of_range_values_before_the_gauge() {
        // The render-isolation path: a hostile f32 (NaN, >1) renders as a
        // plain gauge instead of panicking.
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut state = DashboardState::new("x");
        state.shift.oee = f32::NAN;
        state.shift.a = 1.5;
        let now = Instant::now();
        terminal.draw(|f| ui(f, &state, now)).unwrap();
    }
}
