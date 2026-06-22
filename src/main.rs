use chrono::{Datelike, Duration, NaiveDate, Utc};
use clap::{ArgAction, Parser, Subcommand};
use colored::Colorize;
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitCode, Stdio};
use std::time::SystemTime;

type AppResult<T> = Result<T, String>;

const AGENTS_DIR: &str = ".agents";
const DATA_DIR: &str = ".ntkn";
const DB_FILE: &str = "ntkn.sqlite";
const RULES_FILE: &str = "ntkn-rules.md";
const CLAUDE_HOOK_SCRIPT: &str = "hooks/claude-code/ntkn-record.sh";
const CLAUDE_SETTINGS_FILE: &str = "settings.json";
const CLAUDE_HOOK_SCRIPT_CONTENT: &str = include_str!("../hooks/claude-code/ntkn-record.sh");
const CLAUDE_SETTINGS_CONTENT: &str = include_str!("../hooks/claude-code/settings.json");
const CODEX_HOOK_SCRIPT: &str = "hooks/codex/ntkn-record.sh";
const CODEX_HOOKS_FILE: &str = "hooks.json";
const CODEX_HOOK_SCRIPT_CONTENT: &str = include_str!("../hooks/codex/ntkn-record.sh");
const CODEX_DISPATCH_SCRIPT_CONTENT: &str = include_str!("../hooks/codex/ntkn-dispatch.sh");
const CODEX_GLOBAL_HOOKS_TEMPLATE: &str = include_str!("../hooks/codex/global-hooks.json");
const CODEX_DISPATCH_ARG: &str = "__NTKN_DISPATCH__";
const CURSOR_HOOK_SCRIPT: &str = "hooks/ntkn-record.sh";
const CURSOR_HOOKS_FILE: &str = "hooks.json";
const CURSOR_HOOK_SCRIPT_CONTENT: &str = include_str!("../hooks/cursor/ntkn-record.sh");
const CURSOR_HOOKS_CONTENT: &str = include_str!("../hooks/cursor/hooks.json");
const AGY_HOOK_SCRIPT: &str = "hooks/ntkn-record.sh";
const AGY_HOOKS_FILE: &str = "hooks.json";
const AGY_HOOK_SCRIPT_CONTENT: &str = include_str!("../hooks/agy/ntkn-record.sh");
const AGY_HOOKS_CONTENT: &str = include_str!("../hooks/agy/hooks.json");
const OPENCODE_HOOK_SCRIPT: &str = "hooks/opencode/ntkn-record.sh";
const OPENCODE_PLUGIN_FILE: &str = "plugins/ntkn.js";
const OPENCODE_HOOK_SCRIPT_CONTENT: &str = include_str!("../hooks/opencode/ntkn-record.sh");
const OPENCODE_PLUGIN_CONTENT: &str = include_str!("../hooks/opencode/plugin.js");

#[derive(Parser)]
#[command(
    name = "ntkn",
    version,
    about = "Nub Token : Local Token Tracker for AI Agents",
    arg_required_else_help = false,
    disable_help_flag = true
)]
struct Cli {
    #[arg(short = 'h', long = "help", action = ArgAction::SetTrue)]
    help: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize ntkn in the current directory.
    Init {
        #[arg(long)]
        project: String,
    },
    /// Record one token usage event.
    Record {
        #[arg(long)]
        project: String,
        #[arg(long, default_value = "manual")]
        provider: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        prompt: i64,
        #[arg(long = "comp")]
        completion: i64,
        #[arg(long)]
        duration: Option<i64>,
    },
    /// Show token usage totals for the current project.
    Usage,
    /// Show project setup and hook health.
    Status,
    /// Show recent usage events for the current project.
    History {
        #[arg(long, default_value_t = 10)]
        limit: i64,
    },
    /// Show token usage activity heatmap.
    Stats,
    /// Reset usage stats for the current project.
    Reset,
    /// Reset hook sync state for the current project.
    Clean,
    /// Pull Claude Code token usage from the latest transcript for this project.
    SyncClaude,
    /// Pull Codex token usage from the latest session JSONL for this project.
    SyncCodex,
    /// Replay the Cursor stop hook against the latest agent transcript for this project.
    SyncCursor,
    /// Replay the latest Antigravity stop hook capture for this project.
    SyncAgy,
    /// Replay the latest OpenCode session idle event for this project.
    SyncOpencode,
}

struct ModelSummary {
    provider: String,
    model: String,
    prompt: i64,
    completion: i64,
}

struct UsageRecord {
    id: i64,
    provider: String,
    model: String,
    prompt: i64,
    completion: i64,
    timestamp: String,
}

struct StatsSummary {
    all_time: i64,
    last_7_days: i64,
    last_30_days: i64,
    favorite_model: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{} {}", "error:".red().bold(), error);
            ExitCode::FAILURE
        }
    }
}

fn run() -> AppResult<()> {
    let cli = Cli::parse();
    if cli.help {
        print_splash();
        return Ok(());
    }

    match cli.command {
        Some(Command::Init { project }) => init(&project),
        Some(Command::Record {
            project,
            provider,
            model,
            prompt,
            completion,
            duration,
        }) => record(&project, &provider, &model, prompt, completion, duration),
        Some(Command::Usage) => usage(),
        Some(Command::Status) => status(),
        Some(Command::History { limit }) => history(limit),
        Some(Command::Stats) => stats(),
        Some(Command::Reset) => reset_stats(),
        Some(Command::Clean) => clean_state(),
        Some(Command::SyncClaude) => sync_claude(),
        Some(Command::SyncCodex) => sync_codex(),
        Some(Command::SyncCursor) => sync_cursor(),
        Some(Command::SyncAgy) => sync_agy(),
        Some(Command::SyncOpencode) => sync_opencode(),
        None => {
            print_splash();
            Ok(())
        }
    }
}

fn print_splash() {
    println!("{}", "ntkn (นับโทเค็น)".cyan().bold());
    println!(
        "{}",
        format!(
            "Nub Token : Local Token Tracker for AI Agents v{}",
            env!("CARGO_PKG_VERSION")
        )
        .dimmed()
    );
    println!();
    println!("{}", "Usage".bold());
    println!("  ntkn init --project <NAME>");
    println!(
        "  ntkn record --project <PROJ> --provider <TOOL> --model <MODEL> --prompt <NUM> --comp <NUM>"
    );
    println!("  ntkn usage");
    println!("  ntkn status");
    println!("  ntkn stats");
    println!("  ntkn reset");
    println!("  ntkn clean");
    println!("  ntkn sync-claude");
    println!("  ntkn sync-codex");
    println!("  ntkn sync-cursor");
    println!("  ntkn sync-agy");
    println!("  ntkn sync-opencode");
    println!("  ntkn history --limit <NUM>");
    println!("  ntkn -V, --version");
    println!();
    println!("{}", "Data".bold());
    println!("  .ntkn/ntkn.sqlite");
    println!("  .ntkn/rules/ntkn-rules.md");
    println!("  .ntkn/hooks/codex/ntkn-record.sh");
    println!("  .cursor/hooks/ntkn-record.sh");
    println!("  .agy/hooks/ntkn-record.sh");
    println!("  .ntkn/hooks/opencode/ntkn-record.sh");
    println!("  ~/.codex/hooks/ntkn-dispatch.sh");
    println!("  ~/.codex/hooks.json");
    println!("  .claude/settings.json");
    println!("  .cursor/hooks.json");
    println!("  .agy/hooks.json");
    println!("  .opencode/plugins/ntkn.js");
}

fn init(project: &str) -> AppResult<()> {
    validate_required(project, "project")?;

    migrate_legacy_layout()?;

    let rules_dir = rules_dir()?;
    fs::create_dir_all(&rules_dir)
        .map_err(|error| format!("could not create {}: {error}", rules_dir.display()))?;

    let db_path = db_path()?;
    let connection = Connection::open(&db_path)
        .map_err(|error| format!("could not open {}: {error}", db_path.display()))?;
    create_schema(&connection)?;

    let rules_path = rules_path()?;
    if !rules_path.exists() {
        fs::write(&rules_path, default_rules(project))
            .map_err(|error| format!("could not write {}: {error}", rules_path.display()))?;
    }

    let claude_hook_path = install_claude_code_hooks()?;
    let claude_settings_path = install_claude_code_settings()?;
    let codex_hook_path = install_codex_hooks()?;
    let codex_dispatch_path = install_global_codex_hook()?;
    let cursor_hook_path = install_cursor_hooks()?;
    let cursor_hooks_path = install_cursor_hooks_config()?;
    let agy_hook_path = install_agy_hooks()?;
    let agy_hooks_path = install_agy_hooks_config()?;
    let opencode_hook_path = install_opencode_hook()?;
    let opencode_plugin_path = install_opencode_plugin()?;
    warn_about_project_codex_hooks()?;

    println!(
        "{}",
        format!("initialized ntkn for project `{project}`").green()
    );
    println!("{}", format!("database: {}", db_path.display()).dimmed());
    println!("{}", format!("rules: {}", rules_path.display()).dimmed());
    println!(
        "{}",
        format!("claude hook: {}", claude_hook_path.display()).dimmed()
    );
    println!(
        "{}",
        format!("claude settings: {}", claude_settings_path.display()).dimmed()
    );
    println!(
        "{}",
        format!("codex hook: {}", codex_hook_path.display()).dimmed()
    );
    println!(
        "{}",
        format!("codex dispatch: {}", codex_dispatch_path.display()).dimmed()
    );
    println!(
        "{}",
        format!("cursor hook: {}", cursor_hook_path.display()).dimmed()
    );
    println!(
        "{}",
        format!("cursor hooks: {}", cursor_hooks_path.display()).dimmed()
    );
    println!(
        "{}",
        format!("agy hook: {}", agy_hook_path.display()).dimmed()
    );
    println!(
        "{}",
        format!("agy hooks: {}", agy_hooks_path.display()).dimmed()
    );
    println!(
        "{}",
        format!("opencode hook: {}", opencode_hook_path.display()).dimmed()
    );
    println!(
        "{}",
        format!("opencode plugin: {}", opencode_plugin_path.display()).dimmed()
    );
    println!(
        "{}",
        "Codex Desktop has no /hooks command. Run `ntkn sync-codex` after Codex work.".yellow()
    );
    println!(
        "{}",
        "Cursor records usage from stop-hook input_tokens/output_tokens. Run \
         `ntkn sync-cursor` if totals look stale."
            .yellow()
    );
    println!(
        "{}",
        "Antigravity records usage from stop-hook input_tokens/output_tokens. Run \
         `ntkn sync-agy` if totals look stale."
            .yellow()
    );
    println!(
        "{}",
        "OpenCode records usage from its session.idle plugin event when usage metadata is present. \
         Run `ntkn sync-opencode` if totals look stale."
            .yellow()
    );
    println!(
        "{}",
        "Optional auto-recording: run `codex` in Terminal once and approve the startup \
         \"Hooks need review\" prompt (CLI only)."
            .yellow()
    );
    Ok(())
}

fn sync_claude() -> AppResult<()> {
    let project_dir = project_root()?;
    let hook_path = claude_hook_script_path()?;
    if !hook_path.exists() {
        return Err(format!(
            "{} not found; run `ntkn init --project <NAME>` first",
            hook_path.display()
        ));
    }

    let connection = open_existing_connection()?;
    let project_id = read_project_id()?;
    let count_before = usage_row_count(&connection, &project_id)?;
    let session = find_latest_claude_transcript(&project_dir)?;
    let payload = format!(
        r#"{{"session_id":"{}","transcript_path":{},"cwd":{},"hook_event_name":"Stop"}}"#,
        session.session_id,
        json_string(&session.transcript.display().to_string()),
        json_string(&project_dir.display().to_string())
    );

    let mut child = ProcessCommand::new("bash")
        .arg(&hook_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run {}: {error}", hook_path.display()))?;

    child
        .stdin
        .take()
        .ok_or_else(|| "could not write hook payload".to_owned())?
        .write_all(payload.as_bytes())
        .map_err(|error| format!("could not write hook payload: {error}"))?;

    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not wait for {}: {error}", hook_path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "claude sync hook failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    let count_after = usage_row_count(&connection, &project_id)?;
    if count_after == count_before {
        return Err(
            "no new Claude Code usage found. Finish a Claude Code turn first, then run \
             `ntkn sync-claude` again to replay the latest transcript"
                .to_owned(),
        );
    }

    println!(
        "{}",
        format!(
            "synced Claude Code usage from {}",
            session.transcript.display()
        )
        .green()
    );
    usage()
}

fn sync_codex() -> AppResult<()> {
    let project_dir = project_root()?;
    let hook_path = codex_hook_script_path()?;
    if !hook_path.exists() {
        return Err(format!(
            "{} not found; run `ntkn init --project <NAME>` first",
            hook_path.display()
        ));
    }

    let session = find_latest_codex_session(&project_dir)?;
    let payload = format!(
        r#"{{"session_id":"{}","transcript_path":{},"cwd":{},"hook_event_name":"Stop"}}"#,
        session.session_id,
        json_string(&session.transcript.display().to_string()),
        json_string(&project_dir.display().to_string())
    );

    let mut child = ProcessCommand::new("bash")
        .arg(&hook_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run {}: {error}", hook_path.display()))?;

    child
        .stdin
        .take()
        .ok_or_else(|| "could not write hook payload".to_owned())?
        .write_all(payload.as_bytes())
        .map_err(|error| format!("could not write hook payload: {error}"))?;

    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not wait for {}: {error}", hook_path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "codex sync hook failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    println!(
        "{}",
        format!("synced Codex usage from {}", session.transcript.display()).green()
    );
    usage()
}

fn sync_cursor() -> AppResult<()> {
    let project_dir = project_root()?;
    let hook_path = cursor_hook_script_path()?;
    if !hook_path.exists() {
        return Err(format!(
            "{} not found; run `ntkn init --project <NAME>` first",
            hook_path.display()
        ));
    }

    let connection = open_existing_connection()?;
    let project_id = read_project_id()?;
    let count_before = usage_row_count(&connection, &project_id)?;

    let payload_path = data_dir()?.join("cursor-last-payload.json");
    let payload = if payload_path.is_file() {
        fs::read_to_string(&payload_path)
            .map_err(|error| format!("could not read {}: {error}", payload_path.display()))?
    } else {
        let session = find_latest_cursor_transcript(&project_dir)?;
        format!(
            r#"{{"session_id":"{}","transcript_path":{},"cwd":{},"hook_event_name":"stop"}}"#,
            session.session_id,
            json_string(&session.transcript.display().to_string()),
            json_string(&project_dir.display().to_string())
        )
    };

    let mut child = ProcessCommand::new("bash")
        .arg(&hook_path)
        .env("NTKN_FORCE_SYNC", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run {}: {error}", hook_path.display()))?;

    child
        .stdin
        .take()
        .ok_or_else(|| "could not write hook payload".to_owned())?
        .write_all(payload.as_bytes())
        .map_err(|error| format!("could not write hook payload: {error}"))?;

    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not wait for {}: {error}", hook_path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "cursor sync hook failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    let count_after = usage_row_count(&connection, &project_id)?;
    if count_after == count_before {
        return Err(
            "no new Cursor usage found. Finish a Cursor agent turn first so the stop hook \
             captures input_tokens/output_tokens; then run `ntkn sync-cursor` again to replay \
             the last captured stop payload"
                .to_owned(),
        );
    }

    println!("{}", "synced Cursor usage from last stop payload".green());
    usage()
}

fn sync_agy() -> AppResult<()> {
    let hook_path = agy_hook_script_path()?;
    if !hook_path.exists() {
        return Err(format!(
            "{} not found; run `ntkn init --project <NAME>` first",
            hook_path.display()
        ));
    }

    let connection = open_existing_connection()?;
    let project_id = read_project_id()?;
    let count_before = usage_row_count(&connection, &project_id)?;

    let payload_path = data_dir()?.join("agy-last-payload.json");
    if !payload_path.is_file() {
        return Err(format!(
            "{} does not exist. Finish an Antigravity agent turn first so the stop hook captures usage",
            payload_path.display()
        ));
    }
    let payload = fs::read_to_string(&payload_path)
        .map_err(|error| format!("could not read {}: {error}", payload_path.display()))?;

    let mut child = ProcessCommand::new("bash")
        .arg(&hook_path)
        .env("NTKN_FORCE_SYNC", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run {}: {error}", hook_path.display()))?;

    child
        .stdin
        .take()
        .ok_or_else(|| "could not write hook payload".to_owned())?
        .write_all(payload.as_bytes())
        .map_err(|error| format!("could not write hook payload: {error}"))?;

    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not wait for {}: {error}", hook_path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "agy sync hook failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    let count_after = usage_row_count(&connection, &project_id)?;
    if count_after == count_before {
        return Err(
            "no new Antigravity usage found. Finish an Antigravity agent turn first so the stop \
             hook captures input_tokens/output_tokens; then run `ntkn sync-agy` again"
                .to_owned(),
        );
    }

    println!(
        "{}",
        "synced Antigravity usage from last stop payload".green()
    );
    usage()
}

fn sync_opencode() -> AppResult<()> {
    let hook_path = opencode_hook_script_path()?;
    if !hook_path.exists() {
        return Err(format!(
            "{} not found; run `ntkn init --project <NAME>` first",
            hook_path.display()
        ));
    }

    let connection = open_existing_connection()?;
    let project_id = read_project_id()?;
    let count_before = usage_row_count(&connection, &project_id)?;

    let payload_path = data_dir()?.join("opencode-last-event.json");
    if !payload_path.is_file() {
        return Err(format!(
            "{} does not exist. Finish an OpenCode session first so the plugin captures usage",
            payload_path.display()
        ));
    }
    let payload = fs::read_to_string(&payload_path)
        .map_err(|error| format!("could not read {}: {error}", payload_path.display()))?;

    let mut child = ProcessCommand::new("bash")
        .arg(&hook_path)
        .env("NTKN_FORCE_SYNC", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run {}: {error}", hook_path.display()))?;

    child
        .stdin
        .take()
        .ok_or_else(|| "could not write OpenCode payload".to_owned())?
        .write_all(payload.as_bytes())
        .map_err(|error| format!("could not write OpenCode payload: {error}"))?;

    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not wait for {}: {error}", hook_path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "opencode sync hook failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    let count_after = usage_row_count(&connection, &project_id)?;
    if count_after == count_before {
        return Err(
            "no new OpenCode usage found. Finish an OpenCode session first so the plugin \
             captures usage metadata; then run `ntkn sync-opencode` again"
                .to_owned(),
        );
    }

    println!("{}", "synced OpenCode usage from last plugin event".green());
    usage()
}

struct CursorSessionMatch {
    session_id: String,
    transcript: PathBuf,
    modified: SystemTime,
}

struct ClaudeTranscriptMatch {
    session_id: String,
    transcript: PathBuf,
    modified: SystemTime,
}

fn find_latest_claude_transcript(project_dir: &Path) -> AppResult<ClaudeTranscriptMatch> {
    let transcripts_root = claude_home()?
        .join("projects")
        .join(claude_project_slug(project_dir));
    if !transcripts_root.is_dir() {
        return Err(format!(
            "{} does not exist; run Claude Code in this project first",
            transcripts_root.display()
        ));
    }

    let mut best: Option<ClaudeTranscriptMatch> = None;
    collect_claude_transcripts(&transcripts_root, &mut best)?;

    best.ok_or_else(|| {
        format!(
            "no Claude Code transcript found for {}; run Claude Code in this project first",
            project_dir.display()
        )
    })
}

fn collect_claude_transcripts(
    dir: &Path,
    best: &mut Option<ClaudeTranscriptMatch>,
) -> AppResult<()> {
    for entry in
        fs::read_dir(dir).map_err(|error| format!("could not read {}: {error}", dir.display()))?
    {
        let entry =
            entry.map_err(|error| format!("could not read {} entry: {error}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }

        let Some(session_id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };

        let modified = entry
            .metadata()
            .map_err(|error| format!("could not read {} metadata: {error}", path.display()))?
            .modified()
            .map_err(|error| format!("could not read {} modified time: {error}", path.display()))?;

        if best
            .as_ref()
            .map(|current| modified > current.modified)
            .unwrap_or(true)
        {
            *best = Some(ClaudeTranscriptMatch {
                session_id: session_id.to_owned(),
                transcript: path,
                modified,
            });
        }
    }

    Ok(())
}

fn find_latest_cursor_transcript(project_dir: &Path) -> AppResult<CursorSessionMatch> {
    let slug = cursor_project_slug(project_dir);
    let transcripts_root = cursor_home()?
        .join("projects")
        .join(slug)
        .join("agent-transcripts");
    if !transcripts_root.is_dir() {
        return Err(format!(
            "{} does not exist; run Cursor in this project first",
            transcripts_root.display()
        ));
    }

    let mut best: Option<CursorSessionMatch> = None;
    collect_cursor_transcripts(&transcripts_root, &mut best)?;

    best.ok_or_else(|| {
        format!(
            "no Cursor transcript found for {}; run Cursor in this project first",
            project_dir.display()
        )
    })
}

fn collect_cursor_transcripts(dir: &Path, best: &mut Option<CursorSessionMatch>) -> AppResult<()> {
    for entry in
        fs::read_dir(dir).map_err(|error| format!("could not read {}: {error}", dir.display()))?
    {
        let entry =
            entry.map_err(|error| format!("could not read {} entry: {error}", dir.display()))?;
        let session_dir = entry.path();
        if !session_dir.is_dir() {
            continue;
        }

        let Some(session_id) = session_dir.file_name().and_then(|value| value.to_str()) else {
            continue;
        };

        let transcript = session_dir.join(format!("{session_id}.jsonl"));
        if !transcript.is_file() {
            continue;
        }

        let modified = entry
            .metadata()
            .map_err(|error| format!("could not read {} metadata: {error}", session_dir.display()))?
            .modified()
            .map_err(|error| {
                format!(
                    "could not read {} modified time: {error}",
                    session_dir.display()
                )
            })?;

        if best
            .as_ref()
            .map(|current| modified > current.modified)
            .unwrap_or(true)
        {
            *best = Some(CursorSessionMatch {
                session_id: session_id.to_owned(),
                transcript,
                modified,
            });
        }
    }

    Ok(())
}

fn cursor_home() -> AppResult<PathBuf> {
    if let Ok(home) = env::var("CURSOR_HOME") {
        return Ok(PathBuf::from(home));
    }

    let home =
        env::var("HOME").map_err(|error| format!("could not resolve home directory: {error}"))?;
    Ok(PathBuf::from(home).join(".cursor"))
}

fn claude_home() -> AppResult<PathBuf> {
    if let Ok(home) = env::var("CLAUDE_HOME") {
        return Ok(PathBuf::from(home));
    }

    let home =
        env::var("HOME").map_err(|error| format!("could not resolve home directory: {error}"))?;
    Ok(PathBuf::from(home).join(".claude"))
}

fn claude_project_slug(project_dir: &Path) -> String {
    let path = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    path.display().to_string().replace('/', "-")
}

fn cursor_project_slug(project_dir: &Path) -> String {
    let path = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    path.display()
        .to_string()
        .trim_start_matches('/')
        .replace('/', "-")
}

fn read_project_id() -> AppResult<String> {
    let path = rules_path()?;
    let content = fs::read_to_string(&path).map_err(|error| {
        format!(
            "could not read {}; run `ntkn init --project <NAME>` first: {error}",
            path.display()
        )
    })?;

    for line in content.lines() {
        if let Some(value) = line.trim().strip_prefix("project_id:") {
            return Ok(frontmatter_value(value));
        }
    }

    Err(format!("{} is missing project_id", path.display()))
}

fn usage_row_count(connection: &Connection, project_id: &str) -> AppResult<i64> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM usage WHERE project_id = ?1",
            params![project_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("could not count usage rows: {error}"))
}

struct CodexSessionMatch {
    session_id: String,
    transcript: PathBuf,
    modified: SystemTime,
}

fn find_latest_codex_session(project_dir: &Path) -> AppResult<CodexSessionMatch> {
    let sessions_root = codex_home()?.join("sessions");
    if !sessions_root.is_dir() {
        return Err(format!(
            "{} does not exist; run Codex in this project first",
            sessions_root.display()
        ));
    }

    let cwd = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let cwd = cwd.display().to_string();
    let mut best: Option<CodexSessionMatch> = None;
    collect_codex_sessions(&sessions_root, &cwd, &mut best)?;

    best.ok_or_else(|| {
        format!(
            "no Codex session found for {}; run Codex in this project first",
            project_dir.display()
        )
    })
}

fn collect_codex_sessions(
    dir: &Path,
    cwd: &str,
    best: &mut Option<CodexSessionMatch>,
) -> AppResult<()> {
    for entry in
        fs::read_dir(dir).map_err(|error| format!("could not read {}: {error}", dir.display()))?
    {
        let entry =
            entry.map_err(|error| format!("could not read {} entry: {error}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_codex_sessions(&path, cwd, best)?;
            continue;
        }

        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }

        let Some(session_id) = read_codex_session_id(&path, cwd)? else {
            continue;
        };

        let modified = entry
            .metadata()
            .map_err(|error| format!("could not read {} metadata: {error}", path.display()))?
            .modified()
            .map_err(|error| format!("could not read {} modified time: {error}", path.display()))?;

        if best
            .as_ref()
            .map(|current| modified > current.modified)
            .unwrap_or(true)
        {
            *best = Some(CodexSessionMatch {
                session_id,
                transcript: path,
                modified,
            });
        }
    }

    Ok(())
}

fn read_codex_session_id(path: &Path, cwd: &str) -> AppResult<Option<String>> {
    let file = fs::File::open(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;

    if !line.contains("\"session_meta\"") {
        return Ok(None);
    }

    if extract_json_string_field(&line, "cwd")?.as_deref() != Some(cwd) {
        return Ok(None);
    }

    let Some(id) = extract_json_string_field(&line, "id")? else {
        return Ok(None);
    };

    Ok(Some(id))
}

fn extract_json_string_field(line: &str, field: &str) -> AppResult<Option<String>> {
    let marker = format!(r#""{field}":"#);
    let Some(start) = line.find(&marker) else {
        return Ok(None);
    };

    let value_start = start + marker.len();
    let bytes = line.as_bytes();
    if value_start >= bytes.len() || bytes[value_start] != b'"' {
        return Ok(None);
    }

    let mut value = String::new();
    let mut index = value_start + 1;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '\\' {
            index += 1;
            if index >= bytes.len() {
                return Err(format!("invalid escape in {field} field"));
            }
            value.push(bytes[index] as char);
        } else if ch == '"' {
            return Ok(Some(value));
        } else {
            value.push(ch);
        }
        index += 1;
    }

    Err(format!("unterminated {field} field"))
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

fn migrate_legacy_layout() -> AppResult<()> {
    let root = project_root()?;
    let legacy_dir = root.join(AGENTS_DIR);
    let data_dir = root.join(DATA_DIR);
    fs::create_dir_all(&data_dir)
        .map_err(|error| format!("could not create {}: {error}", data_dir.display()))?;

    let legacy_db = legacy_dir.join(DB_FILE);
    let data_db = data_dir.join(DB_FILE);
    if legacy_db.exists() && !data_db.exists() {
        fs::copy(&legacy_db, &data_db).map_err(|error| {
            format!(
                "could not migrate {} to {}: {error}",
                legacy_db.display(),
                data_db.display()
            )
        })?;
    }

    let legacy_rules = legacy_dir.join("rules").join(RULES_FILE);
    let data_rules = data_dir.join("rules").join(RULES_FILE);
    if legacy_rules.exists() && !data_rules.exists() {
        if let Some(parent) = data_rules.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        fs::copy(&legacy_rules, &data_rules).map_err(|error| {
            format!(
                "could not migrate {} to {}: {error}",
                legacy_rules.display(),
                data_rules.display()
            )
        })?;
    }

    Ok(())
}

fn record(
    project: &str,
    provider: &str,
    model: &str,
    prompt: i64,
    completion: i64,
    duration: Option<i64>,
) -> AppResult<()> {
    validate_required(project, "project")?;
    validate_required(provider, "provider")?;
    validate_required(model, "model")?;
    validate_tokens(prompt, "prompt")?;
    validate_tokens(completion, "comp")?;
    let duration = match duration {
        Some(value) => value,
        None => default_duration_ms()?,
    };
    validate_tokens(duration, "duration")?;
    let total = add_tokens(prompt, completion)?;

    let connection = open_existing_connection()?;
    let now = Utc::now();
    let timestamp = now.to_rfc3339();
    let timestamp_unix_ms = now.timestamp_millis();

    connection
        .execute(
            "INSERT INTO usage
                (project_id, provider, model_name, prompt_tokens, completion_tokens, duration_ms, timestamp, timestamp_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                project,
                provider,
                model,
                prompt,
                completion,
                duration,
                timestamp,
                timestamp_unix_ms
            ],
        )
        .map_err(|error| format!("could not record usage: {error}"))?;

    println!(
        "{}",
        format!("recorded {} tokens", format_tokens(total)).dimmed()
    );
    Ok(())
}

fn usage() -> AppResult<()> {
    let project = current_project_id()?;
    let connection = open_existing_connection()?;
    let mut statement = connection
        .prepare(
            "SELECT provider, model_name, SUM(prompt_tokens), SUM(completion_tokens)
             FROM usage
             WHERE project_id = ?1
             GROUP BY provider, model_name
             ORDER BY SUM(prompt_tokens + completion_tokens) DESC, provider, model_name",
        )
        .map_err(|error| format!("could not query status: {error}"))?;

    let rows = statement
        .query_map(params![project], |row| {
            Ok(ModelSummary {
                provider: row.get(0)?,
                model: row.get(1)?,
                prompt: row.get(2)?,
                completion: row.get(3)?,
            })
        })
        .map_err(|error| format!("could not query status: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not read status rows: {error}"))?;

    if rows.is_empty() {
        println!("{}", "no usage recorded yet".dimmed());
        return Ok(());
    }

    let prompt_total = sum_tokens(rows.iter().map(|row| row.prompt))?;
    let completion_total = sum_tokens(rows.iter().map(|row| row.completion))?;
    let grand_total = add_tokens(prompt_total, completion_total)?;

    let mut table = base_table();
    table.set_header(vec!["Provider", "Model", "Prompt", "Completion", "Total"]);
    for row in rows {
        let total = add_tokens(row.prompt, row.completion)?;
        table.add_row(vec![
            Cell::new(row.provider),
            Cell::new(row.model),
            Cell::new(format_tokens(row.prompt)),
            Cell::new(format_tokens(row.completion)),
            Cell::new(format_tokens(total)),
        ]);
    }
    table.add_row(vec![
        Cell::new("Grand Total").add_attribute(Attribute::Bold),
        Cell::new("-").add_attribute(Attribute::Bold),
        Cell::new(format_tokens(prompt_total)).add_attribute(Attribute::Bold),
        Cell::new(format_tokens(completion_total)).add_attribute(Attribute::Bold),
        Cell::new(format_tokens(grand_total)).add_attribute(Attribute::Bold),
    ]);

    println!("{table}");
    Ok(())
}

fn status() -> AppResult<()> {
    let root = project_root()?;
    let rules = rules_path()?;
    let db = db_path()?;
    let project = read_project_id().ok();
    let row_count = match &project {
        Some(project) if db.exists() => {
            let connection = open_existing_connection()?;
            usage_row_count(&connection, project)?
        }
        _ => 0,
    };
    let default_duration = default_duration_ms().unwrap_or(0);

    let claude_hook = claude_hook_script_path()?;
    let claude_settings = claude_settings_dir()?.join(CLAUDE_SETTINGS_FILE);
    let codex_hook = codex_hook_script_path()?;
    let codex_dispatch = codex_home()?.join("hooks").join("ntkn-dispatch.sh");
    let codex_hooks = codex_home()?.join(CODEX_HOOKS_FILE);
    let cursor_hook = cursor_hook_script_path()?;
    let cursor_hooks = cursor_hooks_dir()?.join(CURSOR_HOOKS_FILE);
    let agy_hook = agy_hook_script_path()?;
    let agy_hooks = agy_hooks_dir()?.join(AGY_HOOKS_FILE);
    let opencode_hook = opencode_hook_script_path()?;
    let opencode_plugin = opencode_plugin_path()?;

    let mut table = base_table();
    table.set_header(vec!["Check", "Status", "Value"]);
    table.add_row(status_row("Project root", true, root.display().to_string()));
    table.add_row(status_row(
        "Project id",
        project.is_some(),
        project.unwrap_or_else(|| "missing; run `ntkn init --project <NAME>`".to_owned()),
    ));
    table.add_row(status_row(
        "Database",
        db.exists(),
        db.display().to_string(),
    ));
    table.add_row(status_row(
        "Rules",
        rules.exists(),
        rules.display().to_string(),
    ));
    table.add_row(status_row("Usage rows", true, row_count.to_string()));
    table.add_row(status_row(
        "Default duration",
        true,
        format!("{default_duration} ms"),
    ));
    table.add_row(status_row(
        "Claude hook",
        claude_hook.exists(),
        claude_hook.display().to_string(),
    ));
    table.add_row(status_row(
        "Claude settings",
        claude_settings.exists(),
        claude_settings.display().to_string(),
    ));
    table.add_row(status_row(
        "Codex hook",
        codex_hook.exists(),
        codex_hook.display().to_string(),
    ));
    table.add_row(status_row(
        "Codex dispatcher",
        codex_dispatch.exists(),
        codex_dispatch.display().to_string(),
    ));
    table.add_row(status_row(
        "Codex hooks",
        codex_hooks.exists(),
        codex_hooks.display().to_string(),
    ));
    table.add_row(status_row(
        "Cursor hook",
        cursor_hook.exists(),
        cursor_hook.display().to_string(),
    ));
    table.add_row(status_row(
        "Cursor hooks",
        cursor_hooks.exists(),
        cursor_hooks.display().to_string(),
    ));
    table.add_row(status_row(
        "Antigravity hook",
        agy_hook.exists(),
        agy_hook.display().to_string(),
    ));
    table.add_row(status_row(
        "Antigravity hooks",
        agy_hooks.exists(),
        agy_hooks.display().to_string(),
    ));
    table.add_row(status_row(
        "OpenCode hook",
        opencode_hook.exists(),
        opencode_hook.display().to_string(),
    ));
    table.add_row(status_row(
        "OpenCode plugin",
        opencode_plugin.exists(),
        opencode_plugin.display().to_string(),
    ));

    println!("{table}");
    if !db.exists() || !rules.exists() {
        println!(
            "{}",
            "run `ntkn init --project <NAME>` in this project to install ntkn".yellow()
        );
    }
    Ok(())
}

fn status_row(label: &str, ok: bool, value: String) -> Vec<Cell> {
    let status = if ok {
        Cell::new("ok").fg(Color::Green)
    } else {
        Cell::new("missing").fg(Color::Yellow)
    };

    vec![Cell::new(label), status, Cell::new(value)]
}

fn stats() -> AppResult<()> {
    let project = current_project_id()?;
    let connection = open_existing_connection()?;
    let today = Utc::now().date_naive();
    let first_day = today - Duration::days(364);
    let start = first_day - Duration::days(first_day.weekday().num_days_from_sunday() as i64);

    let daily = daily_usage(&connection, &project, start)?;
    let summary = stats_summary(&connection, &project, today)?;
    print_heatmap(start, today, &daily);

    println!();
    println!(
        "  {} {} {} {} {}",
        format!("All time {}", format_compact_tokens(summary.all_time)).green(),
        "·".dimmed(),
        format!("Last 7 days {}", format_compact_tokens(summary.last_7_days)).green(),
        "·".dimmed(),
        format!(
            "Last 30 days {}",
            format_compact_tokens(summary.last_30_days)
        )
        .green()
    );
    println!();
    println!(
        "  {} {}         {} {}",
        "Favorite model:".dimmed(),
        summary
            .favorite_model
            .unwrap_or_else(|| "-".to_owned())
            .green(),
        "Total tokens:".dimmed(),
        format_compact_tokens(summary.all_time).green().bold()
    );
    Ok(())
}

fn daily_usage(
    connection: &Connection,
    project: &str,
    start: NaiveDate,
) -> AppResult<HashMap<NaiveDate, i64>> {
    let mut statement = connection
        .prepare(
            "SELECT date(timestamp_unix_ms / 1000, 'unixepoch') AS day,
                    SUM(prompt_tokens + completion_tokens)
             FROM usage
             WHERE project_id = ?1
               AND timestamp_unix_ms >= ?2
             GROUP BY day",
        )
        .map_err(|error| format!("could not query stats: {error}"))?;

    let start_ms = day_start_ms(start)?;
    let rows = statement
        .query_map(params![project, start_ms], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| format!("could not query stats: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not read stats rows: {error}"))?;

    let mut daily = HashMap::new();
    for (day, total) in rows {
        let day = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
            .map_err(|error| format!("could not parse stats date {day}: {error}"))?;
        daily.insert(day, total);
    }
    Ok(daily)
}

fn stats_summary(
    connection: &Connection,
    project: &str,
    today: NaiveDate,
) -> AppResult<StatsSummary> {
    let last_7_start = day_start_ms(today - Duration::days(6))?;
    let last_30_start = day_start_ms(today - Duration::days(29))?;
    let all_time = sum_since(connection, project, 0)?;
    let last_7_days = sum_since(connection, project, last_7_start)?;
    let last_30_days = sum_since(connection, project, last_30_start)?;
    let favorite_model = favorite_model(connection, project)?;

    Ok(StatsSummary {
        all_time,
        last_7_days,
        last_30_days,
        favorite_model,
    })
}

fn sum_since(connection: &Connection, project: &str, start_ms: i64) -> AppResult<i64> {
    connection
        .query_row(
            "SELECT COALESCE(SUM(prompt_tokens + completion_tokens), 0)
             FROM usage
             WHERE project_id = ?1
               AND timestamp_unix_ms >= ?2",
            params![project, start_ms],
            |row| row.get(0),
        )
        .map_err(|error| format!("could not query stats totals: {error}"))
}

fn favorite_model(connection: &Connection, project: &str) -> AppResult<Option<String>> {
    let mut statement = connection
        .prepare(
            "SELECT model_name
             FROM usage
             WHERE project_id = ?1
             GROUP BY model_name
             ORDER BY SUM(prompt_tokens + completion_tokens) DESC, model_name
             LIMIT 1",
        )
        .map_err(|error| format!("could not query favorite model: {error}"))?;

    let mut rows = statement
        .query_map(params![project], |row| row.get::<_, String>(0))
        .map_err(|error| format!("could not query favorite model: {error}"))?;

    match rows.next() {
        Some(row) => row
            .map(Some)
            .map_err(|error| format!("could not read favorite model: {error}")),
        None => Ok(None),
    }
}

fn print_heatmap(start: NaiveDate, today: NaiveDate, daily: &HashMap<NaiveDate, i64>) {
    let weeks = ((today - start).num_days() / 7 + 1) as usize;
    let max = daily.values().copied().max().unwrap_or(0);

    println!("{}", month_header(today).green());
    for weekday in 0..7 {
        let label = match weekday {
            1 => "  Mon ",
            3 => "  Wed ",
            5 => "  Fri ",
            _ => "      ",
        };
        print!("{label}");
        for week in 0..weeks {
            let day = start + Duration::days((week * 7 + weekday) as i64);
            if day > today {
                print!(" ");
                continue;
            }
            print!("{}", heat_cell(*daily.get(&day).unwrap_or(&0), max));
        }
        println!();
    }
    println!();
    println!(
        "      {} {} {} {} {}",
        "Less".dimmed(),
        heat_cell(1, 4),
        heat_cell(2, 4),
        heat_cell(3, 4),
        format!("{} More", heat_cell(4, 4)).dimmed()
    );
}

fn month_header(today: NaiveDate) -> String {
    let mut labels = Vec::new();
    let start = today.month() as i32 - 12;
    for offset in 0..=12 {
        let month = (start + offset - 1).rem_euclid(12) + 1;
        labels.push(month_name(month as u32));
    }
    format!("      {}", labels.join(" "))
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "",
    }
}

fn heat_cell(total: i64, max: i64) -> colored::ColoredString {
    match heat_level(total, max) {
        0 => "·".green().dimmed(),
        1 => "░".green(),
        2 => "▒".bright_green(),
        3 => "▓".truecolor(80, 200, 120),
        _ => "█".truecolor(0, 180, 90).bold(),
    }
}

fn heat_level(total: i64, max: i64) -> i64 {
    if total <= 0 || max <= 0 {
        return 0;
    }
    ((total * 4 + max - 1) / max).clamp(1, 4)
}

fn day_start_ms(day: NaiveDate) -> AppResult<i64> {
    day.and_hms_opt(0, 0, 0)
        .map(|value| value.and_utc().timestamp_millis())
        .ok_or_else(|| format!("could not build start of day for {day}"))
}

fn history(limit: i64) -> AppResult<()> {
    validate_limit(limit)?;

    let project = current_project_id()?;
    let connection = open_existing_connection()?;
    let mut statement = connection
        .prepare(
            "SELECT id, provider, model_name, prompt_tokens, completion_tokens, timestamp
             FROM usage
             WHERE project_id = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )
        .map_err(|error| format!("could not query history: {error}"))?;

    let rows = statement
        .query_map(params![project, limit], |row| {
            Ok(UsageRecord {
                id: row.get(0)?,
                provider: row.get(1)?,
                model: row.get(2)?,
                prompt: row.get(3)?,
                completion: row.get(4)?,
                timestamp: row.get(5)?,
            })
        })
        .map_err(|error| format!("could not query history: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not read history rows: {error}"))?;

    if rows.is_empty() {
        println!("{}", "no usage recorded yet".dimmed());
        return Ok(());
    }

    let mut table = base_table();
    table.set_header(vec![
        "ID",
        "Timestamp",
        "Provider",
        "Model",
        "Prompt",
        "Completion",
        "Total",
    ]);
    for row in rows {
        let total = add_tokens(row.prompt, row.completion)?;
        table.add_row(vec![
            Cell::new(row.id),
            Cell::new(row.timestamp),
            Cell::new(row.provider),
            Cell::new(row.model),
            Cell::new(format_tokens(row.prompt)),
            Cell::new(format_tokens(row.completion)),
            Cell::new(format_tokens(total)),
        ]);
    }

    println!("{table}");
    Ok(())
}

fn reset_stats() -> AppResult<()> {
    let project = current_project_id()?;
    let connection = open_existing_connection()?;
    let count = usage_row_count(&connection, &project)?;
    if count == 0 {
        println!("{}", "no usage recorded yet".dimmed());
        return Ok(());
    }

    print!(
        "{} ",
        format!("reset {count} usage rows for project `{project}`? type RESET to confirm:")
            .yellow()
    );
    std::io::stdout()
        .flush()
        .map_err(|error| format!("could not write confirmation prompt: {error}"))?;

    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("could not read confirmation: {error}"))?;
    if answer.trim() != "RESET" {
        println!("{}", "reset cancelled".dimmed());
        return Ok(());
    }

    let deleted = connection
        .execute("DELETE FROM usage WHERE project_id = ?1", params![project])
        .map_err(|error| format!("could not reset usage stats: {error}"))?;
    println!("{}", format!("reset {deleted} usage rows").green());
    Ok(())
}

fn clean_state() -> AppResult<()> {
    let dir = data_dir()?;
    let files = [
        ("codex-state.json", r#"{"sessions":{}}"#),
        (
            "cursor-state.json",
            r#"{"sessions":{},"seen_generations":{}}"#,
        ),
        ("agy-state.json", r#"{"sessions":{},"seen_generations":{}}"#),
        ("opencode-state.json", r#"{"seen":{}}"#),
    ];

    print!(
        "{} ",
        "clean hook sync state? type CLEAN to confirm:".yellow()
    );
    std::io::stdout()
        .flush()
        .map_err(|error| format!("could not write confirmation prompt: {error}"))?;

    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("could not read confirmation: {error}"))?;
    if answer.trim() != "CLEAN" {
        println!("{}", "clean cancelled".dimmed());
        return Ok(());
    }

    fs::create_dir_all(&dir)
        .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    for (name, content) in files {
        let path = dir.join(name);
        fs::write(&path, format!("{content}\n"))
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    }

    println!("{}", "cleaned hook sync state".green());
    Ok(())
}

fn create_schema(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id TEXT NOT NULL,
                provider TEXT NOT NULL DEFAULT 'unknown',
                model_name TEXT NOT NULL,
                prompt_tokens INTEGER NOT NULL,
                completion_tokens INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                timestamp TEXT NOT NULL,
                timestamp_unix_ms INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_usage_project_model
                ON usage (project_id, model_name);",
        )
        .map_err(|error| format!("could not initialize database schema: {error}"))?;

    if !usage_has_column(connection, "provider")? {
        connection
            .execute(
                "ALTER TABLE usage ADD COLUMN provider TEXT NOT NULL DEFAULT 'unknown'",
                [],
            )
            .map_err(|error| format!("could not add provider column: {error}"))?;
    }

    connection
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_usage_project_provider_model
                ON usage (project_id, provider, model_name)",
            [],
        )
        .map_err(|error| format!("could not create provider index: {error}"))?;

    if !usage_has_column(connection, "duration_ms")? {
        connection
            .execute(
                "ALTER TABLE usage ADD COLUMN duration_ms INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|error| format!("could not add duration_ms column: {error}"))?;
    }

    if !usage_has_column(connection, "timestamp_unix_ms")? {
        connection
            .execute(
                "ALTER TABLE usage ADD COLUMN timestamp_unix_ms INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|error| format!("could not add timestamp_unix_ms column: {error}"))?;
    }

    connection
        .execute(
            "UPDATE usage
             SET timestamp_unix_ms = CAST(strftime('%s', timestamp) AS INTEGER) * 1000
             WHERE timestamp_unix_ms = 0
               AND strftime('%s', timestamp) IS NOT NULL",
            [],
        )
        .map_err(|error| format!("could not backfill timestamp_unix_ms: {error}"))?;

    Ok(())
}

fn usage_has_column(connection: &Connection, column: &str) -> AppResult<bool> {
    let mut statement = connection
        .prepare("PRAGMA table_info(usage)")
        .map_err(|error| format!("could not inspect database schema: {error}"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("could not inspect database schema: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not read database schema: {error}"))?;

    Ok(columns.iter().any(|name| name == column))
}

fn base_table() -> Table {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table
}

fn current_project_id() -> AppResult<String> {
    let path = rules_path()?;
    let content = fs::read_to_string(&path).map_err(|error| {
        format!(
            "could not read {}; run `ntkn init --project <NAME>` first: {error}",
            path.display()
        )
    })?;

    for line in content.lines() {
        if let Some(value) = line.trim().strip_prefix("project_id:") {
            let project = frontmatter_value(value);
            if project.is_empty() {
                return Err(format!("{} has an empty project_id", path.display()));
            }
            return Ok(project);
        }
    }

    Err(format!("{} is missing project_id", path.display()))
}

fn default_duration_ms() -> AppResult<i64> {
    let path = rules_path()?;
    let content = fs::read_to_string(&path).map_err(|error| {
        format!(
            "could not read {}; run `ntkn init --project <NAME>` first: {error}",
            path.display()
        )
    })?;

    for line in content.lines() {
        if let Some(value) = line.trim().strip_prefix("default_duration_ms:") {
            let value = frontmatter_value(value);
            return value.parse::<i64>().map_err(|error| {
                format!(
                    "{} has invalid default_duration_ms: {error}",
                    path.display()
                )
            });
        }
    }

    Ok(0)
}

fn default_rules(project: &str) -> String {
    format!(
        r#"---
project_id: {}
budget_limit: 100000
default_duration_ms: 0
---

# ntkn Rules

## Token Efficiency

- Keep prompts specific and remove stale context.
- Prefer repo-local evidence over repeated explanation.

## Claude Code

`ntkn init` installs a Stop hook at `.ntkn/hooks/claude-code/ntkn-record.sh`
and wires it from `.claude/settings.json`. After each turn, the hook reads the
session transcript and appends new usage rows to `.ntkn/ntkn.sqlite`.

Requirements:

- `ntkn` on your PATH (`cargo install --path .` from the ntkn repo)
- `jq` installed
- Run `ntkn init --project <name>` once in this repo before starting Claude Code

Check totals with `ntkn usage`.

If totals look stale after Claude Code work, run:

```sh
ntkn sync-claude
```

Usage groups by provider and model, so the same model name used through
different tools stays separate.

## Codex

Codex Desktop has no `/hooks` slash command. The reliable way to refresh totals
after Codex work is:

```sh
ntkn sync-codex
```

`ntkn init` also installs a Stop hook at `.ntkn/hooks/codex/ntkn-record.sh`
and a global dispatcher at `~/.codex/hooks/ntkn-dispatch.sh`. That hook only
runs after Codex trusts it. Codex Desktop does not expose a trust UI; trust once
from the Terminal CLI instead:

```sh
cd /path/to/project
codex
```

At startup, choose **Trust all and continue** when **Hooks need review** appears.
That trust applies to Codex Desktop too.

When working in this repo with Codex, run `ntkn sync-codex` before finishing a
task so token totals stay current.

Requirements:

- `ntkn` on your PATH
- `jq` installed

Check totals with `ntkn usage`.

## Cursor

`ntkn init` installs a Cursor `stop` hook at `.cursor/hooks.json` and
`.cursor/hooks/ntkn-record.sh`. The hook reads per-turn `input_tokens` and
`output_tokens` from the Cursor stop payload (transcripts do not include usage).

If totals look stale after Cursor work, run:

```sh
ntkn sync-cursor
```

That replays the last captured stop payload from `.ntkn/cursor-last-payload.json`.
Finish at least one agent turn first so the stop hook can capture token fields.

Requirements:

- `ntkn` on your PATH
- `jq` installed

Check totals with `ntkn usage`.

## Antigravity

`ntkn init` installs an Antigravity `stop` hook at `.agy/hooks.json` and
`.agy/hooks/ntkn-record.sh`. The hook reads per-turn `input_tokens` and
`output_tokens` from the Antigravity stop payload.

If totals look stale after Antigravity work, run:

```sh
ntkn sync-agy
```

That replays the last captured stop payload from `.ntkn/agy-last-payload.json`.
Finish at least one agent turn first so the stop hook can capture token fields.

Requirements:

- `ntkn` on your PATH
- `jq` installed

Check totals with `ntkn usage`.

## OpenCode

`ntkn init` installs an OpenCode project plugin at `.opencode/plugins/ntkn.js`
and a recorder at `.ntkn/hooks/opencode/ntkn-record.sh`. The plugin listens for
`session.idle` events and records usage when OpenCode includes token metadata.

If totals look stale after OpenCode work, run:

```sh
ntkn sync-opencode
```

That replays the last captured plugin event from `.ntkn/opencode-last-event.json`.
Restart OpenCode after running `ntkn init` so the plugin is loaded.

Requirements:

- `ntkn` on your PATH
- `jq` installed

Check totals with `ntkn usage`.
"#,
        yaml_string(project)
    )
}

fn install_claude_code_hooks() -> AppResult<PathBuf> {
    let hook_path = claude_hook_script_path()?;
    if let Some(parent) = hook_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }

    fs::write(&hook_path, CLAUDE_HOOK_SCRIPT_CONTENT)
        .map_err(|error| format!("could not write {}: {error}", hook_path.display()))?;
    make_executable(&hook_path)?;

    Ok(hook_path)
}

fn install_claude_code_settings() -> AppResult<PathBuf> {
    let settings_dir = claude_settings_dir()?;
    fs::create_dir_all(&settings_dir)
        .map_err(|error| format!("could not create {}: {error}", settings_dir.display()))?;

    let settings_path = settings_dir.join(CLAUDE_SETTINGS_FILE);
    if settings_path.exists() {
        return Ok(settings_path);
    }

    fs::write(&settings_path, CLAUDE_SETTINGS_CONTENT)
        .map_err(|error| format!("could not write {}: {error}", settings_path.display()))?;

    Ok(settings_path)
}

fn claude_hook_script_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join(CLAUDE_HOOK_SCRIPT))
}

fn claude_settings_dir() -> AppResult<PathBuf> {
    Ok(env::current_dir()
        .map_err(|error| format!("could not read current directory: {error}"))?
        .join(".claude"))
}

fn install_codex_hooks() -> AppResult<PathBuf> {
    let hook_path = codex_hook_script_path()?;
    if let Some(parent) = hook_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }

    fs::write(&hook_path, CODEX_HOOK_SCRIPT_CONTENT)
        .map_err(|error| format!("could not write {}: {error}", hook_path.display()))?;
    make_executable(&hook_path)?;

    Ok(hook_path)
}

fn install_global_codex_hook() -> AppResult<PathBuf> {
    let codex_home = codex_home()?;
    let hooks_dir = codex_home.join("hooks");
    fs::create_dir_all(&hooks_dir)
        .map_err(|error| format!("could not create {}: {error}", hooks_dir.display()))?;

    let dispatch_path = hooks_dir.join("ntkn-dispatch.sh");
    fs::write(&dispatch_path, CODEX_DISPATCH_SCRIPT_CONTENT)
        .map_err(|error| format!("could not write {}: {error}", dispatch_path.display()))?;
    make_executable(&dispatch_path)?;

    let hooks_path = codex_home.join(CODEX_HOOKS_FILE);
    let dispatch_arg = dispatch_path.display().to_string();
    let hooks_content = CODEX_GLOBAL_HOOKS_TEMPLATE.replace(CODEX_DISPATCH_ARG, &dispatch_arg);

    if hooks_path.exists() {
        let existing = fs::read_to_string(&hooks_path)
            .map_err(|error| format!("could not read {}: {error}", hooks_path.display()))?;
        if existing.contains("ntkn-dispatch.sh") {
            fs::write(&hooks_path, hooks_content)
                .map_err(|error| format!("could not refresh {}: {error}", hooks_path.display()))?;
            println!(
                "{}",
                "refreshed ~/.codex/hooks.json; if you use the Codex CLI, approve \
                 the startup Hooks need review prompt again when it appears"
                    .yellow()
            );
            return Ok(dispatch_path);
        }

        println!(
            "{}",
            format!(
                "note: {} already exists; merge the Stop hook from hooks/codex/global-hooks.json",
                hooks_path.display()
            )
            .yellow()
        );
        return Ok(dispatch_path);
    }

    fs::write(&hooks_path, hooks_content)
        .map_err(|error| format!("could not write {}: {error}", hooks_path.display()))?;

    Ok(dispatch_path)
}

fn warn_about_project_codex_hooks() -> AppResult<()> {
    let project_hooks = codex_hooks_dir()?.join(CODEX_HOOKS_FILE);
    if !project_hooks.exists() {
        return Ok(());
    }

    println!(
        "{}",
        format!(
            "note: remove {} to avoid double-recording; ntkn uses ~/.codex/hooks.json",
            project_hooks.display()
        )
        .yellow()
    );
    Ok(())
}

fn codex_home() -> AppResult<PathBuf> {
    if let Ok(home) = env::var("CODEX_HOME") {
        return Ok(PathBuf::from(home));
    }

    let home =
        env::var("HOME").map_err(|error| format!("could not resolve home directory: {error}"))?;
    Ok(PathBuf::from(home).join(".codex"))
}

fn codex_hook_script_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join(CODEX_HOOK_SCRIPT))
}

fn codex_hooks_dir() -> AppResult<PathBuf> {
    Ok(env::current_dir()
        .map_err(|error| format!("could not read current directory: {error}"))?
        .join(".codex"))
}

fn install_cursor_hooks() -> AppResult<PathBuf> {
    let hook_path = cursor_hook_script_path()?;
    if let Some(parent) = hook_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }

    fs::write(&hook_path, CURSOR_HOOK_SCRIPT_CONTENT)
        .map_err(|error| format!("could not write {}: {error}", hook_path.display()))?;
    make_executable(&hook_path)?;

    Ok(hook_path)
}

fn install_cursor_hooks_config() -> AppResult<PathBuf> {
    let hooks_dir = cursor_hooks_dir()?;
    fs::create_dir_all(&hooks_dir)
        .map_err(|error| format!("could not create {}: {error}", hooks_dir.display()))?;

    let hooks_path = hooks_dir.join(CURSOR_HOOKS_FILE);
    if hooks_path.exists() {
        let existing = fs::read_to_string(&hooks_path)
            .map_err(|error| format!("could not read {}: {error}", hooks_path.display()))?;
        if existing.contains("ntkn-record.sh") {
            fs::write(&hooks_path, CURSOR_HOOKS_CONTENT)
                .map_err(|error| format!("could not refresh {}: {error}", hooks_path.display()))?;
        } else {
            println!(
                "{}",
                format!(
                    "note: {} already exists; merge the stop hook from hooks/cursor/hooks.json",
                    hooks_path.display()
                )
                .yellow()
            );
        }
        return Ok(hooks_path);
    }

    fs::write(&hooks_path, CURSOR_HOOKS_CONTENT)
        .map_err(|error| format!("could not write {}: {error}", hooks_path.display()))?;

    Ok(hooks_path)
}

fn cursor_hook_script_path() -> AppResult<PathBuf> {
    Ok(cursor_hooks_dir()?.join(CURSOR_HOOK_SCRIPT))
}

fn cursor_hooks_dir() -> AppResult<PathBuf> {
    Ok(env::current_dir()
        .map_err(|error| format!("could not read current directory: {error}"))?
        .join(".cursor"))
}

fn install_agy_hooks() -> AppResult<PathBuf> {
    let hook_path = agy_hook_script_path()?;
    if let Some(parent) = hook_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }

    fs::write(&hook_path, AGY_HOOK_SCRIPT_CONTENT)
        .map_err(|error| format!("could not write {}: {error}", hook_path.display()))?;
    make_executable(&hook_path)?;

    Ok(hook_path)
}

fn install_agy_hooks_config() -> AppResult<PathBuf> {
    let hooks_dir = agy_hooks_dir()?;
    fs::create_dir_all(&hooks_dir)
        .map_err(|error| format!("could not create {}: {error}", hooks_dir.display()))?;

    let hooks_path = hooks_dir.join(AGY_HOOKS_FILE);
    if hooks_path.exists() {
        let existing = fs::read_to_string(&hooks_path)
            .map_err(|error| format!("could not read {}: {error}", hooks_path.display()))?;
        if existing.contains("ntkn-record.sh") {
            fs::write(&hooks_path, AGY_HOOKS_CONTENT)
                .map_err(|error| format!("could not refresh {}: {error}", hooks_path.display()))?;
        } else {
            println!(
                "{}",
                format!(
                    "note: {} already exists; merge the stop hook from hooks/agy/hooks.json",
                    hooks_path.display()
                )
                .yellow()
            );
        }
        return Ok(hooks_path);
    }

    fs::write(&hooks_path, AGY_HOOKS_CONTENT)
        .map_err(|error| format!("could not write {}: {error}", hooks_path.display()))?;

    Ok(hooks_path)
}

fn agy_hook_script_path() -> AppResult<PathBuf> {
    Ok(agy_hooks_dir()?.join(AGY_HOOK_SCRIPT))
}

fn agy_hooks_dir() -> AppResult<PathBuf> {
    Ok(env::current_dir()
        .map_err(|error| format!("could not read current directory: {error}"))?
        .join(".agy"))
}

fn install_opencode_hook() -> AppResult<PathBuf> {
    let hook_path = opencode_hook_script_path()?;
    if let Some(parent) = hook_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }

    fs::write(&hook_path, OPENCODE_HOOK_SCRIPT_CONTENT)
        .map_err(|error| format!("could not write {}: {error}", hook_path.display()))?;
    make_executable(&hook_path)?;

    Ok(hook_path)
}

fn install_opencode_plugin() -> AppResult<PathBuf> {
    let plugin_path = opencode_plugin_path()?;
    if let Some(parent) = plugin_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }

    fs::write(&plugin_path, OPENCODE_PLUGIN_CONTENT)
        .map_err(|error| format!("could not write {}: {error}", plugin_path.display()))?;

    Ok(plugin_path)
}

fn opencode_hook_script_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join(OPENCODE_HOOK_SCRIPT))
}

fn opencode_plugin_path() -> AppResult<PathBuf> {
    Ok(opencode_dir()?.join(OPENCODE_PLUGIN_FILE))
}

fn opencode_dir() -> AppResult<PathBuf> {
    Ok(env::current_dir()
        .map_err(|error| format!("could not read current directory: {error}"))?
        .join(".opencode"))
}

fn make_executable(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)
            .map_err(|error| format!("could not read {} permissions: {error}", path.display()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("could not mark {} executable: {error}", path.display()))?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}

fn yaml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn frontmatter_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return value[1..value.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }
    value.to_owned()
}

fn validate_tokens(value: i64, name: &str) -> AppResult<()> {
    if value < 0 {
        return Err(format!("--{name} must be zero or greater"));
    }
    Ok(())
}

fn validate_required(value: &str, name: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(format!("--{name} cannot be empty"));
    }
    Ok(())
}

fn validate_limit(limit: i64) -> AppResult<()> {
    if limit < 1 {
        return Err("--limit must be at least 1".to_owned());
    }
    Ok(())
}

fn add_tokens(left: i64, right: i64) -> AppResult<i64> {
    left.checked_add(right)
        .ok_or_else(|| "token total is too large".to_owned())
}

fn sum_tokens(mut values: impl Iterator<Item = i64>) -> AppResult<i64> {
    values.try_fold(0_i64, add_tokens)
}

fn open_existing_connection() -> AppResult<Connection> {
    let db_path = existing_db_path()?;
    let connection = Connection::open(&db_path)
        .map_err(|error| format!("could not open {}: {error}", db_path.display()))?;
    create_schema(&connection)?;
    Ok(connection)
}

fn existing_db_path() -> AppResult<PathBuf> {
    let path = db_path()?;
    if !path.exists() {
        return Err(format!(
            "{} does not exist; run `ntkn init --project <NAME>` first",
            path.display()
        ));
    }
    Ok(path)
}

fn db_path() -> AppResult<PathBuf> {
    let preferred = data_dir()?.join(DB_FILE);
    if preferred.exists() {
        return Ok(preferred);
    }

    let legacy = legacy_agents_dir()?.join(DB_FILE);
    if legacy.exists() {
        return Ok(legacy);
    }

    Ok(preferred)
}

fn rules_path() -> AppResult<PathBuf> {
    Ok(rules_dir()?.join(RULES_FILE))
}

fn rules_dir() -> AppResult<PathBuf> {
    let preferred = data_dir()?.join("rules");
    if preferred.join(RULES_FILE).exists() {
        return Ok(preferred);
    }

    let legacy = legacy_agents_dir()?.join("rules");
    if legacy.join(RULES_FILE).exists() {
        return Ok(legacy);
    }

    Ok(preferred)
}

fn data_dir() -> AppResult<PathBuf> {
    if let Ok(dir) = env::var("NTKN_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }

    Ok(project_root()?.join(DATA_DIR))
}

fn legacy_agents_dir() -> AppResult<PathBuf> {
    Ok(project_root()?.join(AGENTS_DIR))
}

fn project_root() -> AppResult<PathBuf> {
    env::current_dir().map_err(|error| format!("could not read current directory: {error}"))
}

fn format_tokens(value: i64) -> String {
    let chars = value.to_string().chars().rev().collect::<Vec<_>>();
    let mut formatted = String::new();
    for (index, ch) in chars.iter().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(*ch);
    }
    formatted.chars().rev().collect()
}

fn format_compact_tokens(value: i64) -> String {
    if value >= 1_000_000 {
        return format_compact_decimal(value, 1_000_000, "m");
    }
    if value >= 1_000 {
        return format_compact_decimal(value, 1_000, "k");
    }
    value.to_string()
}

fn format_compact_decimal(value: i64, unit: i64, suffix: &str) -> String {
    let whole = value / unit;
    let decimal = (value % unit) * 10 / unit;
    if decimal == 0 {
        format!("{whole}{suffix}")
    } else {
        format!("{whole}.{decimal}{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_tokens_with_commas() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(1_234_567), "1,234,567");
    }

    #[test]
    fn formats_compact_tokens() {
        assert_eq!(format_compact_tokens(999), "999");
        assert_eq!(format_compact_tokens(1_200), "1.2k");
        assert_eq!(format_compact_tokens(20_100_000), "20.1m");
    }

    #[test]
    fn calculates_heat_levels() {
        assert_eq!(heat_level(0, 100), 0);
        assert_eq!(heat_level(1, 100), 1);
        assert_eq!(heat_level(50, 100), 2);
        assert_eq!(heat_level(100, 100), 4);
    }

    #[test]
    fn reads_quoted_frontmatter_value() {
        assert_eq!(frontmatter_value(r#""alpha project""#), "alpha project");
        assert_eq!(
            frontmatter_value(r#""a \"quoted\" project""#),
            "a \"quoted\" project"
        );
    }

    #[test]
    fn rejects_token_total_overflow() {
        assert!(add_tokens(i64::MAX, 1).is_err());
    }

    #[test]
    fn cursor_project_slug_replaces_slashes() {
        assert_eq!(
            cursor_project_slug(Path::new("/Users/tom/Projects/GitHub/ntkn")),
            "Users-tom-Projects-GitHub-ntkn"
        );
    }

    #[test]
    fn migrates_old_usage_schema() {
        let connection = Connection::open_in_memory().expect("open sqlite");
        connection
            .execute_batch(
                "CREATE TABLE usage (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    model_name TEXT NOT NULL,
                    prompt_tokens INTEGER NOT NULL,
                    completion_tokens INTEGER NOT NULL,
                    timestamp TEXT NOT NULL
                );",
            )
            .expect("create old schema");

        create_schema(&connection).expect("migrate schema");

        assert!(usage_has_column(&connection, "provider").expect("inspect schema"));
        assert!(usage_has_column(&connection, "duration_ms").expect("inspect schema"));
        assert!(usage_has_column(&connection, "timestamp_unix_ms").expect("inspect schema"));
    }
}
