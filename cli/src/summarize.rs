use anyhow::{Context, Result};
use std::io::{self, Write};

const OLLAMA_URL: &str = "http://127.0.0.1:11434/api/generate";
const MODEL: &str = "llama3.2";

pub struct SummaryInput {
    pub period_label: String, // e.g. "week of 2026-09-28" or "October 2026"
    pub entries: Vec<SummaryEntry>,
}

pub struct SummaryEntry {
    pub date: String,
    pub created_at: String,
    pub duration_secs: u64,
    pub transcript: String,
}

pub fn generate_summary(input: &SummaryInput) -> Result<String> {
    if input.entries.is_empty() {
        return Ok("No journal entries found for this period.".to_string());
    }

    let prompt = build_prompt(input);

    println!(
        "\n🧠  Generating summary for {} ({} entries)...\n",
        input.period_label,
        input.entries.len()
    );

    let response = call_ollama(&prompt)?;
    Ok(response)
}

fn build_prompt(input: &SummaryInput) -> String {
    let mut entries_text = String::new();
    for (i, e) in input.entries.iter().enumerate() {
        let mins = e.duration_secs / 60;
        let secs = e.duration_secs % 60;
        entries_text.push_str(&format!(
            "--- Entry {} | {} | {}m {}s ---\n{}\n\n",
            i + 1,
            e.created_at,
            mins,
            secs,
            e.transcript.trim()
        ));
    }

    format!(
        r#"You are a compassionate, insightful journaling coach reviewing someone's personal voice journal entries. Below are the transcribed entries from {period}.

{entries}

Please provide a thoughtful, honest summary addressing each of these questions based on what was actually said. Be specific — reference real things mentioned. Be warm but don't sugarcoat.

---

**1. What is draining me?**
(What recurring stressors, situations, or people are taking energy away?)

**2. What is having me / energizing me?**
(What is lighting me up, giving me momentum, or making me feel alive?)

**3. What is good for me?**
(What habits, relationships, choices, or patterns seem to be serving me well?)

**4. What should I do?**
(What concrete actions would make the biggest difference right now?)

**5. What did I say I would do but didn't do?**
(Any commitments, intentions, or plans mentioned that haven't happened yet?)

**6. What is in the plan?**
(What upcoming things, goals, or milestones did I mention?)

**7. Emotional tone this period**
(Based on language patterns, word choice, and themes — what was the underlying emotional state? Any notable shifts?)

---

Keep your response concise and actionable. Use bullet points within each section. If there isn't enough information to answer a question, say so briefly."#,
        period = input.period_label,
        entries = entries_text,
    )
}

fn call_ollama(prompt: &str) -> Result<String> {
    let body = serde_json::json!({
        "model": MODEL,
        "prompt": prompt,
        "stream": true
    });

    let body_str = serde_json::to_string(&body)?;

    // Use curl as a subprocess — avoids adding a heavy HTTP client dependency.
    // Stream the response and print tokens as they arrive.
    let mut child = std::process::Command::new("curl")
        .args([
            "-s",
            "-X", "POST",
            OLLAMA_URL,
            "-H", "Content-Type: application/json",
            "-d", &body_str,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Failed to spawn curl — is curl installed?")?;

    let stdout = child.stdout.take().context("No stdout")?;
    let reader = io::BufReader::new(stdout);

    let mut full_response = String::new();
    let stdout_handle = io::stdout();
    let mut out = stdout_handle.lock();

    use io::BufRead;
    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        // Each line is a JSON object: {"model":...,"response":"token","done":false}
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(token) = val.get("response").and_then(|v| v.as_str()) {
                full_response.push_str(token);
                write!(out, "{}", token)?;
                out.flush()?;
            }
            if val.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                break;
            }
            // Handle error from Ollama (e.g. model not found)
            if let Some(err) = val.get("error").and_then(|v| v.as_str()) {
                writeln!(out)?;
                return Err(anyhow::anyhow!("Ollama error: {}", err));
            }
        }
    }

    writeln!(out)?; // final newline

    child.wait()?;

    if full_response.is_empty() {
        return Err(anyhow::anyhow!(
            "Ollama returned no response. Is `ollama serve` running? Try: ollama serve &"
        ));
    }

    Ok(full_response)
}
