use chrono::Utc;
use clap::{Parser, Subcommand};
use colored::Colorize;
use comfy_table::{Attribute, Cell, ContentArrangement, Table, presets::UTF8_FULL};
use rusqlite::{Connection, params};
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

#[derive(Parser)]
#[command(
    name = "ntkn",
    version,
    about = "Nub Token : Local Token Tracker for AI Agents",
    arg_required_else_help = false
)]
struct Cli {
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
    /// Alias for usage.
    Status,
    /// Show recent usage events for the current project.
    History {
        #[arg(long, default_value_t = 10)]
        limit: i64,
    },
    /// Reset usage stats for the current project.
    Reset,
    /// Pull Codex token usage from the latest session JSONL for this project.
    SyncCodex,
    /// Replay the Cursor stop hook against the latest agent transcript for this project.
    SyncCursor,
}

struct ModelSummary {
    provider: String,
    model: String,
    prompt: i64,
    completion: i64,
    duration_ms: i64,
}

struct UsageRecord {
    id: i64,
    provider: String,
    model: String,
    prompt: i64,
    completion: i64,
    timestamp: String,
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
    match Cli::parse().command {
        Some(Command::Init { project }) => init(&project),
        Some(Command::Record {
            project,
            provider,
            model,
            prompt,
            completion,
            duration,
        }) => record(&project, &provider, &model, prompt, completion, duration),
        Some(Command::Usage | Command::Status) => usage(),
        Some(Command::History { limit }) => history(limit),
        Some(Command::Reset) => reset_stats(),
        Some(Command::SyncCodex) => sync_codex(),
        Some(Command::SyncCursor) => sync_cursor(),
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
        "  ntkn record --project <PROJ> --provider <TOOL> --model <MODEL> --prompt <NUM> --comp <NUM> [--duration <MS>]"
    );
    println!("  ntkn usage");
    println!("  ntkn status");
    println!("  ntkn reset");
    println!("  ntkn sync-codex");
    println!("  ntkn sync-cursor");
    println!("  ntkn history --limit <NUM>");
    println!("  ntkn -V, --version");
    println!();
    println!("{}", "Data".bold());
    println!("  .ntkn/ntkn.sqlite");
    println!("  .ntkn/rules/ntkn-rules.md");
    println!("  .ntkn/hooks/codex/ntkn-record.sh");
    println!("  .cursor/hooks/ntkn-record.sh");
    println!("  ~/.codex/hooks/ntkn-dispatch.sh");
    println!("  ~/.codex/hooks.json");
    println!("  .claude/settings.json");
    println!("  .cursor/hooks.json");
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
        "Optional auto-recording: run `codex` in Terminal once and approve the startup \
         \"Hooks need review\" prompt (CLI only)."
            .yellow()
    );
    Ok(())
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

struct CursorSessionMatch {
    session_id: String,
    transcript: PathBuf,
    modified: SystemTime,
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
    let timestamp = Utc::now().to_rfc3339();

    connection
        .execute(
            "INSERT INTO usage
                (project_id, provider, model_name, prompt_tokens, completion_tokens, duration_ms, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                project, provider, model, prompt, completion, duration, timestamp
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
            "SELECT provider, model_name, SUM(prompt_tokens), SUM(completion_tokens), SUM(duration_ms)
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
                duration_ms: row.get(4)?,
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
    let duration_total = sum_duration(rows.iter().map(|row| row.duration_ms))?;
    let grand_total = add_tokens(prompt_total, completion_total)?;

    let mut table = base_table();
    table.set_header(vec![
        "Provider",
        "Model",
        "Prompt",
        "Completion",
        "Total",
        "Total Time",
        "Avg Speed",
    ]);
    for row in rows {
        let total = add_tokens(row.prompt, row.completion)?;
        table.add_row(vec![
            Cell::new(row.provider),
            Cell::new(row.model),
            Cell::new(format_tokens(row.prompt)),
            Cell::new(format_tokens(row.completion)),
            Cell::new(format_tokens(total)),
            Cell::new(format_duration(row.duration_ms)),
            Cell::new(format_speed(total, row.duration_ms)),
        ]);
    }
    table.add_row(vec![
        Cell::new("Grand Total").add_attribute(Attribute::Bold),
        Cell::new("-").add_attribute(Attribute::Bold),
        Cell::new(format_tokens(prompt_total)).add_attribute(Attribute::Bold),
        Cell::new(format_tokens(completion_total)).add_attribute(Attribute::Bold),
        Cell::new(format_tokens(grand_total)).add_attribute(Attribute::Bold),
        Cell::new(format_duration(duration_total)).add_attribute(Attribute::Bold),
        Cell::new(format_speed(grand_total, duration_total)).add_attribute(Attribute::Bold),
    ]);

    println!("{table}");
    Ok(())
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
                timestamp TEXT NOT NULL
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

fn sum_duration(mut values: impl Iterator<Item = i64>) -> AppResult<i64> {
    values.try_fold(0_i64, |total, value| {
        total
            .checked_add(value)
            .ok_or_else(|| "duration total is too large".to_owned())
    })
}

fn format_duration(duration_ms: i64) -> String {
    if duration_ms <= 0 {
        return "0s".to_owned();
    }
    if duration_ms < 1_000 {
        return format!("{duration_ms}ms");
    }
    if duration_ms < 60_000 {
        let seconds = duration_ms as f64 / 1_000.0;
        return if duration_ms % 1_000 == 0 {
            format!("{}s", duration_ms / 1_000)
        } else {
            format!("{seconds:.1}s")
        };
    }

    let seconds = duration_ms / 1_000;
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    if minutes < 60 {
        return format!("{minutes}m {seconds}s");
    }

    let hours = minutes / 60;
    let minutes = minutes % 60;
    format!("{hours}h {minutes}m {seconds}s")
}

fn format_speed(tokens: i64, duration_ms: i64) -> String {
    if duration_ms <= 0 {
        return "-".to_owned();
    }
    format!(
        "{:.0} tok/s",
        tokens as f64 / (duration_ms as f64 / 1_000.0)
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_tokens_with_commas() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(1_234_567), "1,234,567");
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
    fn formats_duration_and_speed() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(5400), "5.4s");
        assert_eq!(format_duration(62_000), "1m 2s");
        assert_eq!(format_speed(1500, 0), "-");
        assert_eq!(format_speed(1500, 10_000), "150 tok/s");
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
    }
}
