use crate::watcher::DaemonState;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    widgets::{Block, Borders, Cell, LineGauge, Paragraph, Row, Table, Clear},
    Frame,
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

pub fn draw(f: &mut Frame, state: &DaemonState, current_dir: &str, show_pause_modal: bool, show_stop_modal: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(7), // Table Matrix
            Constraint::Min(8),    // Gauges
            Constraint::Length(1), // Footer Menu
        ])
        .split(f.area());

    // 1. Header
    let timer_str = format_duration(state.elapsed_seconds);
    let header_text = vec![
        ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(" ntkn ", Style::default().fg(Color::Cyan).bold()),
            ratatui::text::Span::styled("● ", Style::default().fg(if state.status == "Running" { Color::Green } else { Color::Yellow })),
            ratatui::text::Span::raw(format!("Dir: {} | Model: {} | Time: ", current_dir, state.active_model)),
            ratatui::text::Span::styled(timer_str, Style::default().fg(Color::Green).bold()),
        ])
    ];
    let header_paragraph = Paragraph::new(header_text)
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray)));
    f.render_widget(header_paragraph, chunks[0]);

    // 2. Table Matrix
    let gpt4o_pct = (state.openai_gpt4o as f64 / GPT4O_MAX as f64) * 100.0;
    let claude_pct = (state.anthropic_claude as f64 / CLAUDE_MAX as f64) * 100.0;
    let gemini_pct = (state.google_gemini as f64 / GEMINI_MAX as f64) * 100.0;

    let active_model_lower = state.active_model.to_lowercase();
    let is_gpt_active = active_model_lower.contains("gpt") || active_model_lower.contains("openai");
    let is_claude_active = active_model_lower.contains("claude") || active_model_lower.contains("anthropic");
    let is_gemini_active = active_model_lower.contains("gemini") || active_model_lower.contains("google");

    let rows = vec![
        Row::new(vec![
            Cell::from(if is_gpt_active { "* OpenAI" } else { "  OpenAI" }).fg(Color::Green),
            Cell::from("GPT-4o"),
            Cell::from(state.openai_gpt4o.to_string()),
            Cell::from(GPT4O_MAX.to_string()),
            Cell::from(format!("{:.2}%", gpt4o_pct)),
        ]).height(1),
        Row::new(vec![
            Cell::from(if is_claude_active { "* Anthropic" } else { "  Anthropic" }).fg(Color::Magenta),
            Cell::from("Claude 3.5 Sonnet"),
            Cell::from(state.anthropic_claude.to_string()),
            Cell::from(CLAUDE_MAX.to_string()),
            Cell::from(format!("{:.2}%", claude_pct)),
        ]).height(1),
        Row::new(vec![
            Cell::from(if is_gemini_active { "* Google" } else { "  Google" }).fg(Color::Yellow),
            Cell::from("Gemini 1.5/2.0"),
            Cell::from(state.google_gemini.to_string()),
            Cell::from(GEMINI_MAX.to_string()),
            Cell::from(format!("{:.2}%", gemini_pct)),
        ]).height(1),
    ];

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
    .header(Row::new(vec!["Provider", "Model Name", "Tokens", "Max Context", "Occupancy"]))
    .block(Block::default().borders(Borders::ALL).title(" Multi-Model Token Matrix "));
    f.render_widget(table, chunks[1]);

    // 3. Gauges
    let gpt_ratio = (state.openai_gpt4o as f64 / GPT4O_MAX as f64).min(1.0).max(0.0);
    let claude_ratio = (state.anthropic_claude as f64 / CLAUDE_MAX as f64).min(1.0).max(0.0);
    let gemini_ratio = (state.google_gemini as f64 / GEMINI_MAX as f64).min(1.0).max(0.0);

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

    let gauge_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2), Constraint::Length(2)])
        .split(chunks[2]);
    f.render_widget(gpt_gauge, gauge_chunks[0]);
    f.render_widget(claude_gauge, gauge_chunks[1]);
    f.render_widget(gemini_gauge, gauge_chunks[2]);

    // 4. Footer Menu
    let footer_text = Paragraph::new("[p] Pause | [s] Stop | [q] Exit (keeps counting)");
    f.render_widget(footer_text, chunks[3]);

    // Modal Confirmation Dialogs
    if show_pause_modal || show_stop_modal {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area); // Clear background under the modal

        let title = if show_pause_modal { " Pause Counting " } else { " Stop Monitoring " };
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
