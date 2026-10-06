mod db;
mod record;
mod summarize;
mod transcribe;
mod video;

use anyhow::{bail, Context, Result};
use chrono::{Datelike, Duration, Local, NaiveDate};
use clap::{Parser, Subcommand};
use std::io::{self, Read};
use std::sync::mpsc;

#[derive(Parser)]
#[command(name = "your-voice", about = "A personal audio journal")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Record a new journal entry
    Record,
    /// Show journal entries for a given date
    Show {
        /// Date to show entries for (YYYY-MM-DD). Defaults to today.
        #[arg(long, value_name = "YYYY-MM-DD")]
        date: Option<String>,
    },
    /// Generate an AI summary of your journal entries
    Summarize {
        /// Summarize a single day (YYYY-MM-DD). Defaults to today.
        #[arg(long, value_name = "YYYY-MM-DD", conflicts_with_all = ["week", "month", "from"])]
        day: Option<Option<String>>,
        /// Summarize the current week (Mon–today)
        #[arg(long, conflicts_with_all = ["month", "day"])]
        week: bool,
        /// Summarize the current month
        #[arg(long, conflicts_with_all = ["week", "day"])]
        month: bool,
        /// Summarize a custom date range (YYYY-MM-DD)
        #[arg(long, value_name = "YYYY-MM-DD", conflicts_with_all = ["week", "month", "day"])]
        from: Option<String>,
        /// End date for custom range (YYYY-MM-DD). Defaults to today.
        #[arg(long, value_name = "YYYY-MM-DD", requires = "from")]
        to: Option<String>,
    },
    /// (Debug) Run transcription on an existing WAV file and print the result
    TestTranscribe {
        /// Path to the WAV file to transcribe
        path: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let conn = db::init()?;

    match cli.command {
        Commands::Record => {
            println!("Journal type:");
            println!("  [a] Audio  (default)");
            println!("  [v] Video");

            let (tx, rx) = mpsc::channel::<u8>();
            std::thread::spawn(move || {
                let mut buf = [0u8; 1];
                if io::stdin().read(&mut buf).is_ok() {
                    let _ = tx.send(buf[0]);
                }
            });
            let choice = rx.recv_timeout(std::time::Duration::from_secs(3))
                .unwrap_or(b'a');

            if choice == b'v' || choice == b'V' {
                let (video_path, tmp_wav, duration_secs) = video::record_video()?;
                println!("Transcribing...");
                let transcript = transcribe::transcribe(&tmp_wav)?;
                let _ = std::fs::remove_file(&tmp_wav);
                println!("\nTranscript:\n{transcript}");
                let id = db::insert_entry(&conn, &video_path, duration_secs, &transcript)?;
                println!("\nEntry saved (id: {id})");
            } else {
                let (path, duration_secs) = record::record_audio()?;
                println!("Transcribing...");
                let transcript = transcribe::transcribe(&path)?;
                println!("\nTranscript:\n{transcript}");
                let id = db::insert_entry(&conn, &path, duration_secs, &transcript)?;
                println!("\nEntry saved (id: {id})");
            }
        }

        Commands::Show { date } => {
            let date_str = date.unwrap_or_else(|| Local::now().format("%Y-%m-%d").to_string());
            let entries = db::get_entries(&conn, &date_str)?;

            if entries.is_empty() {
                println!("No entries found for {date_str}.");
            } else {
                println!("Journal entries for {date_str}");
                println!("{}", "─".repeat(50));
                for (i, entry) in entries.iter().enumerate() {
                    let mins = entry.duration_secs / 60;
                    let secs = entry.duration_secs % 60;
                    println!(
                        "[{}] {} | duration: {}m {}s",
                        i + 1,
                        entry.created_at,
                        mins,
                        secs
                    );
                    println!("    file: {}", entry.file_path);
                    println!("    {}", entry.transcript);
                    if i + 1 < entries.len() {
                        println!();
                    }
                }
                println!("{}", "─".repeat(50));
                println!(
                    "{} entr{} total.",
                    entries.len(),
                    if entries.len() == 1 { "y" } else { "ies" }
                );
            }
        }

        Commands::Summarize { day, week, month, from, to } => {
            let today = Local::now().date_naive();

            let (start_date, end_date, label) = if let Some(day_val) = day {
                let date = match day_val {
                    Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                        .with_context(|| format!("Invalid --day date: {s}"))?,
                    None => today,
                };
                let label = date.format("%Y-%m-%d").to_string();
                (date, date, label)
            } else if week {
                // Monday of the current week → today
                let days_since_monday = today.weekday().num_days_from_monday();
                let monday = today - Duration::days(days_since_monday as i64);
                let label = format!("week of {}", monday.format("%Y-%m-%d"));
                (monday, today, label)
            } else if month {
                let start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
                    .expect("valid date");
                let label = today.format("%B %Y").to_string();
                (start, today, label)
            } else if let Some(from_str) = from {
                let start = NaiveDate::parse_from_str(&from_str, "%Y-%m-%d")
                    .with_context(|| format!("Invalid --from date: {from_str}"))?;
                let end = match to {
                    Some(ref s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
                        .with_context(|| format!("Invalid --to date: {s}"))?,
                    None => today,
                };
                if end < start {
                    bail!("--to date must be on or after --from date");
                }
                let label = format!(
                    "{} to {}",
                    start.format("%Y-%m-%d"),
                    end.format("%Y-%m-%d")
                );
                (start, end, label)
            } else {
                // Default: current week
                let days_since_monday = today.weekday().num_days_from_monday();
                let monday = today - Duration::days(days_since_monday as i64);
                let label = format!("week of {}", monday.format("%Y-%m-%d"));
                (monday, today, label)
            };

            let start_str = start_date.format("%Y-%m-%d").to_string();
            let end_str = end_date.format("%Y-%m-%d").to_string();

            let entries = db::get_entries_for_range(&conn, &start_str, &end_str)?;

            let summary_entries: Vec<summarize::SummaryEntry> = entries
                .into_iter()
                .map(|e| summarize::SummaryEntry {
                    date: e.date,
                    created_at: e.created_at,
                    duration_secs: e.duration_secs,
                    transcript: e.transcript,
                })
                .collect();

            let input = summarize::SummaryInput {
                period_label: label,
                entries: summary_entries,
            };

            summarize::generate_summary(&input)?;
        }

        Commands::TestTranscribe { path } => {
            println!("Running transcription on: {path}");
            let transcript = transcribe::transcribe(&path)?;
            println!("\nTranscript:\n{transcript}");
        }
    }

    Ok(())
}

