use chrono::Utc;
use clap::{Parser, Subcommand};
use colored::Colorize;
use comfy_table::{Attribute, Cell, ContentArrangement, Table, presets::UTF8_FULL};
use rusqlite::{Connection, params};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

type AppResult<T> = Result<T, String>;

const AGENTS_DIR: &str = ".agents";
const DB_FILE: &str = "ntkn.sqlite";
const RULES_FILE: &str = "ntkn-rules.md";

#[derive(Parser)]
#[command(name = "ntkn", version, about = "Local token tracker for AI agents")]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
        #[arg(long)]
        model: String,
        #[arg(long)]
        prompt: i64,
        #[arg(long = "comp")]
        completion: i64,
        #[arg(long, default_value_t = 0)]
        duration: i64,
    },
    /// Show token totals for the current project.
    Status,
    /// Show recent usage events for the current project.
    History {
        #[arg(long, default_value_t = 10)]
        limit: i64,
    },
}

struct ModelSummary {
    model: String,
    prompt: i64,
    completion: i64,
    duration_ms: i64,
}

struct UsageRecord {
    id: i64,
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
        Command::Init { project } => init(&project),
        Command::Record {
            project,
            model,
            prompt,
            completion,
            duration,
        } => record(&project, &model, prompt, completion, duration),
        Command::Status => status(),
        Command::History { limit } => history(limit),
    }
}

fn init(project: &str) -> AppResult<()> {
    validate_required(project, "project")?;

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

    println!(
        "{}",
        format!("initialized ntkn for project `{project}`").green()
    );
    println!("{}", format!("database: {}", db_path.display()).dimmed());
    println!("{}", format!("rules: {}", rules_path.display()).dimmed());
    Ok(())
}

fn record(
    project: &str,
    model: &str,
    prompt: i64,
    completion: i64,
    duration: i64,
) -> AppResult<()> {
    validate_required(project, "project")?;
    validate_required(model, "model")?;
    validate_tokens(prompt, "prompt")?;
    validate_tokens(completion, "comp")?;
    validate_tokens(duration, "duration")?;
    let total = add_tokens(prompt, completion)?;

    let connection = open_existing_connection()?;
    let timestamp = Utc::now().to_rfc3339();

    connection
        .execute(
            "INSERT INTO usage
                (project_id, model_name, prompt_tokens, completion_tokens, duration_ms, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![project, model, prompt, completion, duration, timestamp],
        )
        .map_err(|error| format!("could not record usage: {error}"))?;

    println!(
        "{}",
        format!("recorded {} tokens", format_tokens(total)).dimmed()
    );
    Ok(())
}

fn status() -> AppResult<()> {
    let project = current_project_id()?;
    let connection = open_existing_connection()?;
    let mut statement = connection
        .prepare(
            "SELECT model_name, SUM(prompt_tokens), SUM(completion_tokens), SUM(duration_ms)
             FROM usage
             WHERE project_id = ?1
             GROUP BY model_name
             ORDER BY SUM(prompt_tokens + completion_tokens) DESC, model_name",
        )
        .map_err(|error| format!("could not query status: {error}"))?;

    let rows = statement
        .query_map(params![project], |row| {
            Ok(ModelSummary {
                model: row.get(0)?,
                prompt: row.get(1)?,
                completion: row.get(2)?,
                duration_ms: row.get(3)?,
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
            "SELECT id, model_name, prompt_tokens, completion_tokens, timestamp
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
                model: row.get(1)?,
                prompt: row.get(2)?,
                completion: row.get(3)?,
                timestamp: row.get(4)?,
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
            Cell::new(row.model),
            Cell::new(format_tokens(row.prompt)),
            Cell::new(format_tokens(row.completion)),
            Cell::new(format_tokens(total)),
        ]);
    }

    println!("{table}");
    Ok(())
}

fn create_schema(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id TEXT NOT NULL,
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

fn default_rules(project: &str) -> String {
    format!(
        r#"---
project_id: {}
budget_limit: 100000
---

# ntkn Rules

## Token Efficiency

- Keep prompts specific and remove stale context.
- Prefer repo-local evidence over repeated explanation.
- Record prompt and completion tokens after each agent run.
"#,
        yaml_string(project)
    )
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
    Ok(agents_dir()?.join(DB_FILE))
}

fn rules_path() -> AppResult<PathBuf> {
    Ok(rules_dir()?.join(RULES_FILE))
}

fn rules_dir() -> AppResult<PathBuf> {
    Ok(agents_dir()?.join("rules"))
}

fn agents_dir() -> AppResult<PathBuf> {
    Ok(env::current_dir()
        .map_err(|error| format!("could not read current directory: {error}"))?
        .join(AGENTS_DIR))
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

        assert!(usage_has_column(&connection, "duration_ms").expect("inspect schema"));
    }
}
