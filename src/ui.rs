use crate::counter::TokenMatrix;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    widgets::{Block, Borders, Cell, LineGauge, Paragraph, Row, Table},
    Frame,
};

// Model context window limits
pub const GPT4O_MAX: usize = 128_000;
pub const CLAUDE_MAX: usize = 200_000;
pub const GEMINI_MAX: usize = 1_000_000; // 1M tokens standard limit

/// Helper to format large numbers with commas as thousand separators.
pub fn format_with_commas(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;
    for c in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }
    result.chars().rev().collect()
}

pub fn draw(f: &mut Frame, state: &TokenMatrix, current_dir: &str) {
    // 1. Overall layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(7), // Table
            Constraint::Min(8),    // Gauges
            Constraint::Length(1), // Footer
        ])
        .split(f.area());

    // 2. Header Block
    let header_text = vec![
        ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(" ntkn ", Style::default().fg(Color::Cyan).bold()),
            ratatui::text::Span::styled("●", Style::default().fg(Color::Green)),
            ratatui::text::Span::raw(" Scanned Directory: "),
            ratatui::text::Span::styled(current_dir, Style::default().fg(Color::White).italic()),
        ])
    ];
    let header_paragraph = Paragraph::new(header_text)
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray)));
    f.render_widget(header_paragraph, chunks[0]);

    // 3. Multi-Model Table Matrix
    let gpt4o_pct = (state.openai_gpt4o as f64 / GPT4O_MAX as f64) * 100.0;
    let claude_pct = (state.anthropic_claude as f64 / CLAUDE_MAX as f64) * 100.0;
    let gemini_pct = (state.google_gemini as f64 / GEMINI_MAX as f64) * 100.0;

    let header_cells = ["Provider", "Model Name", "Tokens Count", "Max Context", "Occupancy"]
        .into_iter()
        .map(|h| Cell::from(h).fg(Color::Cyan).bold());
    let header = Row::new(header_cells)
        .style(Style::default().bg(Color::Rgb(30, 30, 40)))
        .height(1);

    let rows = vec![
        Row::new(vec![
            Cell::from("OpenAI").fg(Color::Green),
            Cell::from("GPT-4o"),
            Cell::from(format_with_commas(state.openai_gpt4o)),
            Cell::from(format_with_commas(GPT4O_MAX)),
            Cell::from(format!("{:.2}%", gpt4o_pct)).fg(if gpt4o_pct > 80.0 { Color::Red } else { Color::Green }),
        ]).height(1),
        Row::new(vec![
            Cell::from("Anthropic").fg(Color::Magenta),
            Cell::from("Claude 3.5 Sonnet"),
            Cell::from(format_with_commas(state.anthropic_claude)),
            Cell::from(format_with_commas(CLAUDE_MAX)),
            Cell::from(format!("{:.2}%", claude_pct)).fg(if claude_pct > 80.0 { Color::Red } else { Color::Green }),
        ]).height(1),
        Row::new(vec![
            Cell::from("Google").fg(Color::Yellow),
            Cell::from("Gemini 1.5/2.0"),
            Cell::from(format_with_commas(state.google_gemini)),
            Cell::from(format_with_commas(GEMINI_MAX)),
            Cell::from(format!("{:.2}%", gemini_pct)).fg(if gemini_pct > 80.0 { Color::Red } else { Color::Green }),
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
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Multi-Model Token Matrix ").border_style(Style::default().fg(Color::DarkGray)));
    f.render_widget(table, chunks[1]);

    // 4. Context Window Occupancy Gauges
    let gauges_block = Block::default()
        .borders(Borders::ALL)
        .title(" Context Window Occupancy ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner_gauges_area = gauges_block.inner(chunks[2]);
    f.render_widget(gauges_block, chunks[2]);

    let gauge_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .split(inner_gauges_area);

    // GPT-4o Gauge
    let gpt_ratio = (state.openai_gpt4o as f64 / GPT4O_MAX as f64).min(1.0).max(0.0);
    let gpt_gauge = LineGauge::default()
        .block(Block::default().title("GPT-4o (128k max)"))
        .filled_style(Style::default().fg(Color::Cyan).bg(Color::Rgb(40, 40, 40)))
        .ratio(gpt_ratio)
        .label(format!("{:.2}%", gpt_ratio * 100.0));
    f.render_widget(gpt_gauge, gauge_chunks[0]);

    // Claude Gauge
    let claude_ratio = (state.anthropic_claude as f64 / CLAUDE_MAX as f64).min(1.0).max(0.0);
    let claude_gauge = LineGauge::default()
        .block(Block::default().title("Claude 3.5 Sonnet (200k max)"))
        .filled_style(Style::default().fg(Color::Magenta).bg(Color::Rgb(40, 40, 40)))
        .ratio(claude_ratio)
        .label(format!("{:.2}%", claude_ratio * 100.0));
    f.render_widget(claude_gauge, gauge_chunks[1]);

    // Gemini Gauge
    let gemini_ratio = (state.google_gemini as f64 / GEMINI_MAX as f64).min(1.0).max(0.0);
    let gemini_gauge = LineGauge::default()
        .block(Block::default().title("Gemini 1.5/2.0 (1M max)"))
        .filled_style(Style::default().fg(Color::Yellow).bg(Color::Rgb(40, 40, 40)))
        .ratio(gemini_ratio)
        .label(format!("{:.2}%", gemini_ratio * 100.0));
    f.render_widget(gemini_gauge, gauge_chunks[2]);

    // 5. Footer Menu
    let footer_text = vec![
        ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(" [q] ", Style::default().fg(Color::Red).bold()),
            ratatui::text::Span::raw("Quit   "),
            ratatui::text::Span::styled(" [r] ", Style::default().fg(Color::Yellow).bold()),
            ratatui::text::Span::raw("Rescan"),
        ])
    ];
    let footer_paragraph = Paragraph::new(footer_text)
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(footer_paragraph, chunks[3]);
}
