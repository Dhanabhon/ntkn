use std::{io, panic, time::Duration};
use crossterm::{
    execute,
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    event::{self, Event, KeyCode, KeyModifiers, KeyEventKind},
    cursor,
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod scanner;
mod counter;
mod ui;

type DestructibleTerminal = Terminal<CrosstermBackend<io::Stdout>>;

/// Initializes the terminal by enabling raw mode, entering the alternate screen, and hiding the cursor.
fn setup_terminal() -> Result<DestructibleTerminal, io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

/// Restores the terminal to its original state (disables raw mode, leaves the alternate screen, and shows the cursor).
fn restore_terminal_state() -> Result<(), io::Error> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, cursor::Show)?;
    Ok(())
}

/// Runs the main application loop, drawing the UI and polling for crossterm keyboard events.
fn run_app(terminal: &mut DestructibleTerminal) -> Result<(), io::Error> {
    let current_dir = std::env::current_dir()?;
    let current_dir_str = current_dir.to_string_lossy().to_string();

    let mut project_text = scanner::ProjectScanner::scan_project(&current_dir);
    let mut token_data = counter::TokenCounter::calculate_all(&project_text);

    loop {
        terminal.draw(|f| {
            ui::draw(f, &token_data, &current_dir_str);
        })?;

        // Wait up to 250ms for keyboard events to prevent 100% CPU utilization
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                // Handle key presses (on Windows/enhanced terminals we ignore Release/Repeat events)
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => {
                            break;
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break;
                        }
                        KeyCode::Char('r') => {
                            project_text = scanner::ProjectScanner::scan_project(&current_dir);
                            token_data = counter::TokenCounter::calculate_all(&project_text);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), io::Error> {
    // Set up a custom panic hook to guarantee that the terminal state is restored if the program crashes.
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal_state();
        original_hook(panic_info);
    }));

    let mut terminal = setup_terminal()?;
    let run_result = run_app(&mut terminal);

    // Restore standard terminal state
    restore_terminal_state()?;

    run_result
}