use crate::watcher::DaemonState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    widgets::{Block, Borders, Cell, Clear, LineGauge, Paragraph, Row, Table},
};

pub const GPT4O_MAX: usize = 128_000;
pub const CLAUDE_MAX: usize = 200_000;
pub const GEMINI_MAX: usize = 1_000_000;

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

pub fn draw(
    f: &mut Frame,
    state: &DaemonState,
    current_dir: &str,
    show_pause_modal: bool,
    show_stop_modal: bool,
) {
    let num_rows =
        (state.show_openai as u16) + (state.show_anthropic as u16) + (state.show_gemini as u16);
    let table_height = num_rows + 4; // Header row + borders/spacing

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),            // Header
            Constraint::Length(table_height), // Dynamic Table Matrix
            Constraint::Min(4),               // Gauges
            Constraint::Length(1),            // Footer Menu
        ])
        .split(f.area());

    // 1. Header
    let timer_str = format_duration(state.elapsed_seconds);
    let header_text = vec![ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(" ntkn ", Style::default().fg(Color::Cyan).bold()),
        ratatui::text::Span::styled(
            "● ",
            Style::default().fg(if state.status == "Running" {
                Color::Green
            } else {
                Color::Yellow
            }),
        ),
        ratatui::text::Span::raw(format!(
            "Dir: {} | Model: {} | Time: ",
            current_dir, state.active_model
        )),
        ratatui::text::Span::styled(timer_str, Style::default().fg(Color::Green).bold()),
    ])];
    let header_paragraph = Paragraph::new(header_text).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(header_paragraph, chunks[0]);

    // 2. Table Matrix
    let gpt4o_pct = (state.openai_gpt4o as f64 / GPT4O_MAX as f64) * 100.0;
    let claude_pct = (state.anthropic_claude as f64 / CLAUDE_MAX as f64) * 100.0;
    let gemini_pct = (state.google_gemini as f64 / GEMINI_MAX as f64) * 100.0;

    let active_model_lower = state.active_model.to_lowercase();
    let is_gpt_active = active_model_lower.contains("gpt") || active_model_lower.contains("openai");
    let is_claude_active =
        active_model_lower.contains("claude") || active_model_lower.contains("anthropic");
    let is_gemini_active =
        active_model_lower.contains("gemini") || active_model_lower.contains("google");

    let mut rows = Vec::new();

    if state.show_openai {
        rows.push(
            Row::new(vec![
                Cell::from(if is_gpt_active {
                    "* OpenAI"
                } else {
                    "  OpenAI"
                })
                .fg(Color::Green),
                Cell::from("GPT-4o"),
                Cell::from(state.openai_gpt4o.to_string()),
                Cell::from(GPT4O_MAX.to_string()),
                Cell::from(format!("{:.2}%", gpt4o_pct)),
            ])
            .height(1),
        );
    }

    if state.show_anthropic {
        rows.push(
            Row::new(vec![
                Cell::from(if is_claude_active {
                    "* Anthropic"
                } else {
                    "  Anthropic"
                })
                .fg(Color::Magenta),
                Cell::from("Claude 3.5 Sonnet"),
                Cell::from(state.anthropic_claude.to_string()),
                Cell::from(CLAUDE_MAX.to_string()),
                Cell::from(format!("{:.2}%", claude_pct)),
            ])
            .height(1),
        );
    }

    if state.show_gemini {
        rows.push(
            Row::new(vec![
                Cell::from(if is_gemini_active {
                    "* Google"
                } else {
                    "  Google"
                })
                .fg(Color::Yellow),
                Cell::from("Gemini 1.5/2.0"),
                Cell::from(state.google_gemini.to_string()),
                Cell::from(GEMINI_MAX.to_string()),
                Cell::from(format!("{:.2}%", gemini_pct)),
            ])
            .height(1),
        );
    }

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(30),
            Constraint::Percentage(20),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
        ],
    )
    .header(Row::new(vec![
        "Provider",
        "Model Name",
        "Tokens",
        "Max Context",
        "Occupancy",
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Multi-Model Token Matrix "),
    );
    f.render_widget(table, chunks[1]);

    // 3. Gauges
    let gpt_ratio = (state.openai_gpt4o as f64 / GPT4O_MAX as f64).clamp(0.0, 1.0);
    let claude_ratio = (state.anthropic_claude as f64 / CLAUDE_MAX as f64).clamp(0.0, 1.0);
    let gemini_ratio = (state.google_gemini as f64 / GEMINI_MAX as f64).clamp(0.0, 1.0);

    let gpt_gauge = LineGauge::default()
        .block(Block::default().title("GPT-4o (128k)"))
        .filled_style(Style::default().fg(Color::Cyan))
        .ratio(gpt_ratio);
    let claude_gauge = LineGauge::default()
        .block(Block::default().title("Claude 3.5 Sonnet (200k)"))
        .filled_style(Style::default().fg(Color::Magenta))
        .ratio(claude_ratio);
    let gemini_gauge = LineGauge::default()
        .block(Block::default().title("Gemini 1.5 (1M)"))
        .filled_style(Style::default().fg(Color::Yellow))
        .ratio(gemini_ratio);

    let mut gauge_constraints = Vec::new();
    if state.show_openai {
        gauge_constraints.push(Constraint::Length(2));
    }
    if state.show_anthropic {
        gauge_constraints.push(Constraint::Length(2));
    }
    if state.show_gemini {
        gauge_constraints.push(Constraint::Length(2));
    }

    let gauge_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(gauge_constraints)
        .split(chunks[2]);

    let mut current_gauge_idx = 0;
    if state.show_openai {
        f.render_widget(gpt_gauge, gauge_chunks[current_gauge_idx]);
        current_gauge_idx += 1;
    }
    if state.show_anthropic {
        f.render_widget(claude_gauge, gauge_chunks[current_gauge_idx]);
        current_gauge_idx += 1;
    }
    if state.show_gemini {
        f.render_widget(gemini_gauge, gauge_chunks[current_gauge_idx]);
    }

    // 4. Footer Menu
    let footer_text = Paragraph::new("[p] Pause | [s] Stop | [q] Exit (keeps counting)");
    f.render_widget(footer_text, chunks[3]);

    // Modal Confirmation Dialogs
    if show_pause_modal || show_stop_modal {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area); // Clear background under the modal

        let title = if show_pause_modal {
            " Pause Counting "
        } else {
            " Stop Monitoring "
        };
        let msg = if show_pause_modal {
            "Are you sure you want to pause counting? (y/n)"
        } else {
            "Are you sure you want to stop monitoring? (y/n)"
        };

        let modal_block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Red));

        let p = Paragraph::new(msg)
            .block(modal_block)
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(p, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::DaemonState;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_ui_draw_does_not_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = DaemonState {
            pid: 1234,
            status: "Running".to_string(),
            start_time: 100,
            elapsed_seconds: 45,
            last_updated: 200,
            active_model: "GPT-4o".to_string(),
            model_detected: true,
            openai_gpt4o: 1000,
            anthropic_claude: 2000,
            google_gemini: 3000,
            show_openai: true,
            show_anthropic: true,
            show_gemini: true,
        };

        terminal
            .draw(|f| {
                draw(f, &state, "/test/path", false, false);
            })
            .unwrap();
    }

    #[test]
    fn test_ui_draw_no_providers_does_not_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = DaemonState {
            pid: 1234,
            status: "Running".to_string(),
            start_time: 100,
            elapsed_seconds: 45,
            last_updated: 200,
            active_model: "Unknown".to_string(),
            model_detected: false,
            openai_gpt4o: 0,
            anthropic_claude: 0,
            google_gemini: 0,
            show_openai: false,
            show_anthropic: false,
            show_gemini: false,
        };

        terminal
            .draw(|f| {
                draw(f, &state, "/test/path", false, false);
            })
            .unwrap();
    }

    #[test]
    fn test_ui_draw_small_terminal_does_not_panic() {
        let backend = TestBackend::new(0, 0);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = DaemonState {
            pid: 1234,
            status: "Running".to_string(),
            start_time: 100,
            elapsed_seconds: 45,
            last_updated: 200,
            active_model: "GPT-4o".to_string(),
            model_detected: true,
            openai_gpt4o: 1000,
            anthropic_claude: 2000,
            google_gemini: 3000,
            show_openai: true,
            show_anthropic: true,
            show_gemini: true,
        };

        terminal
            .draw(|f| {
                draw(f, &state, "/test/path", true, true);
            })
            .unwrap();
    }
}
