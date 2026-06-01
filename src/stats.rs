use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct StatsRecord {
    pub timestamp: u64,
    pub openai: usize,
    pub claude: usize,
    pub gemini: usize,
}

pub fn log_historical_stats(path: &Path, record: &StatsRecord) -> Result<(), std::io::Error> {
    let history_file = crate::daemon::get_state_file_path(path).with_file_name(format!(
        "{}-history.json",
        crate::config::get_path_hash(path)
    ));

    let mut records: Vec<StatsRecord> = if history_file.exists() {
        let content = fs::read_to_string(&history_file)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    // Avoid logging duplicates if values didn't change
    if let Some(last) = records.last() {
        if last.openai == record.openai
            && last.claude == record.claude
            && last.gemini == record.gemini
        {
            return Ok(());
        }
    }

    records.push(record.clone());

    let content_updated = serde_json::to_string_pretty(&records)?;
    let temp_path = history_file.with_extension("tmp");
    fs::write(&temp_path, content_updated)?;
    fs::rename(temp_path, &history_file)?;
    Ok(())
}

pub fn view_stats_chart(path: &Path) -> Result<(), std::io::Error> {
    let history_file = crate::daemon::get_state_file_path(path).with_file_name(format!(
        "{}-history.json",
        crate::config::get_path_hash(path)
    ));

    if !history_file.exists() {
        println!("No historical stats found for this project. Start monitoring first.");
        return Ok(());
    }

    let content = fs::read_to_string(&history_file)?;
    let records: Vec<StatsRecord> = serde_json::from_str(&content).unwrap_or_default();

    if records.is_empty() {
        println!("History is empty.");
        return Ok(());
    }

    let latest = &records[records.len() - 1];
    println!("=== Token Usage Stats (Latest Distribution) ===");
    println!("OpenAI GPT-4o:     {}", latest.openai);
    println!("Claude 3.5 Sonnet: {}", latest.claude);
    println!("Gemini 1.5/2.0:    {}", latest.gemini);

    let max = latest.openai.max(latest.claude.max(latest.gemini)) as f64;
    let render_bar = |val: usize| -> String {
        if max == 0.0 {
            return String::new();
        }
        let width = ((val as f64 / max) * 40.0) as usize;
        "█".repeat(width)
    };

    println!("\nOpenAI:  [{:<40}]", render_bar(latest.openai));
    println!("Claude:  [{:<40}]", render_bar(latest.claude));
    println!("Gemini:  [{:<40}]", render_bar(latest.gemini));

    Ok(())
}
