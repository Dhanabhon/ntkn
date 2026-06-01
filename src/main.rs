use std::{io, path::PathBuf, time::Duration};
use crossterm::{
    execute,
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    event::{self, Event, KeyCode, KeyModifiers, KeyEventKind},
    cursor,
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod config;
mod scanner;
mod counter;
mod ui;
mod daemon;
mod watcher;
mod stats;

fn main() -> Result<(), io::Error> {
    let args: Vec<String> = std::env::args().collect();
    
    // Command line sub-routing
    if args.len() > 1 {
        match args[1].as_str() {
            "daemon" => {
                if args.len() > 3 && args[2] == "--watch" {
                    let path = PathBuf::from(&args[3]);
                    return daemon::run_daemon(path);
                }
            }
            "pause" => {
                let current_dir = std::env::current_dir()?;
                return daemon::modify_daemon_status(&current_dir, "Paused");
            }
            "resume" => {
                let current_dir = std::env::current_dir()?;
                return daemon::modify_daemon_status(&current_dir, "Running");
            }
            "stop" => {
                let current_dir = std::env::current_dir()?;
                return daemon::modify_daemon_status(&current_dir, "Stopped");
            }
            "stats" | "usage" => {
                let current_dir = std::env::current_dir()?;
                return stats::view_stats_chart(&current_dir);
            }
            _ => {}
        }
    }

    // Default Interactive Startup
    let current_dir = std::env::current_dir()?;
    if !config::verify_trust_interactive(&current_dir)? {
        println!("Exiting.");
        return Ok(());
    }

    // Spawn daemon if not running
    daemon::spawn_daemon(&current_dir)?;

    // Start TUI Monitor
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, cursor::Show);
        original_hook(panic_info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(err) = execute!(stdout, EnterAlternateScreen, cursor::Hide) {
        let _ = disable_raw_mode();
        return Err(err);
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(err) => {
            let _ = disable_raw_mode();
            let mut stdout = io::stdout();
            let _ = execute!(stdout, LeaveAlternateScreen, cursor::Show);
            return Err(err);
        }
    };

    let current_dir_str = current_dir.to_string_lossy().to_string();
    let state_file = daemon::get_state_file_path(&current_dir);

    let mut show_pause_modal = false;
    let mut show_stop_modal = false;

    loop {
        // Read daemon state from local state JSON
        let state = if state_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&state_file) {
                serde_json::from_str::<watcher::DaemonState>(&content).unwrap_or_else(|_| create_fallback_state())
            } else {
                create_fallback_state()
            }
        } else {
            create_fallback_state()
        };

        terminal.draw(|f| {
            ui::draw(f, &state, &current_dir_str, show_pause_modal, show_stop_modal);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if show_pause_modal {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                let _ = daemon::modify_daemon_status(&current_dir, "Paused");
                                show_pause_modal = false;
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                show_pause_modal = false;
                            }
                            _ => {}
                        }
                    } else if show_stop_modal {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                let _ = daemon::modify_daemon_status(&current_dir, "Stopped");
                                break;
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                show_stop_modal = false;
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') => {
                                break;
                            }
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                break;
                            }
                            KeyCode::Char('p') => {
                                show_pause_modal = true;
                            }
                            KeyCode::Char('s') => {
                                show_stop_modal = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show)?;
    
    println!("ntkn is still counting in the background.");
    println!("To pause counting: run 'ntkn pause'");
    println!("To stop monitoring: run 'ntkn stop'");
    Ok(())
}

fn create_fallback_state() -> watcher::DaemonState {
    watcher::DaemonState {
        pid: 0,
        status: "Running".to_string(),
        start_time: 0,
        elapsed_seconds: 0,
        last_updated: 0,
        active_model: "Loading...".to_string(),
        model_detected: false,
        openai_gpt4o: 0,
        anthropic_claude: 0,
        google_gemini: 0,
        show_openai: true,
        show_anthropic: true,
        show_gemini: true,
    }
}