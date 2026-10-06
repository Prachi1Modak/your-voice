use chrono::Local;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use serde::Serialize;
use tauri::{Emitter, Manager};
use uuid::Uuid;

#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
pub async fn save_recording(
    app_handle: tauri::AppHandle,
    data: Vec<u8>,
    ext: String,
) -> Result<String, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    let media_dir = app_data_dir.join("media");
    fs::create_dir_all(&media_dir).map_err(|e| e.to_string())?;

    let date = Local::now().format("%Y-%m-%d").to_string();
    let filename = format!("{}_{}.{}", date, Uuid::new_v4(), ext);
    let file_path = media_dir.join(&filename);

    fs::write(&file_path, &data).map_err(|e| e.to_string())?;

    file_path
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Invalid path".to_string())
}

#[tauri::command]
pub async fn compress_media(input_path: String, media_type: String) -> Result<String, String> {
    let output_path = if media_type == "audio" {
        if let Some(stem) = std::path::Path::new(&input_path).file_stem() {
            let parent = std::path::Path::new(&input_path)
                .parent()
                .unwrap_or(std::path::Path::new(""));
            parent
                .join(format!("{}.opus", stem.to_string_lossy()))
                .to_string_lossy()
                .into_owned()
        } else {
            format!("{}.opus", input_path)
        }
    } else {
        let path = std::path::Path::new(&input_path);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parent = path.parent().unwrap_or(std::path::Path::new(""));
        parent
            .join(format!("{}_compressed.{}", stem, ext))
            .to_string_lossy()
            .into_owned()
    };

    let input_clone = input_path.clone();
    let output_clone = output_path.clone();
    let media_type_clone = media_type.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("ffmpeg");
        if media_type_clone == "audio" {
            cmd.args(["-i", &input_clone, "-c:a", "libopus", "-b:a", "64k", "-y", &output_clone]);
        } else {
            cmd.args(["-i", &input_clone, "-c:v", "libvpx-vp9", "-b:v", "1200k",
                      "-crf", "30", "-c:a", "libopus", "-b:a", "96k", "-y", &output_clone]);
        }
        cmd.output()
    })
    .await
    .map_err(|e| format!("spawn_blocking error: {}", e))?;

    match result {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(input_path),
        Err(e) => Err(format!("Failed to launch ffmpeg: {}", e)),
        Ok(output) => {
            if output.status.success() {
                Ok(output_path)
            } else {
                Err(String::from_utf8_lossy(&output.stderr).into_owned())
            }
        }
    }
}

#[derive(Serialize, Clone)]
struct TranscriptionCompletePayload {
    entry_id: String,
    transcript: String,
}

#[derive(serde::Deserialize)]
struct WhisperSegment {
    text: String,
}

#[derive(serde::Deserialize)]
struct WhisperOutput {
    transcription: Vec<WhisperSegment>,
}

#[tauri::command]
pub async fn transcribe_audio(
    audio_path: String,
    entry_id: String,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let bundled_bin = app_handle
        .path()
        .resource_dir()
        .ok()
        .map(|p| p.join("bin").join("whisper"));

    let whisper_bin: Option<PathBuf> = match bundled_bin {
        Some(ref p) if p.exists() => Some(p.clone()),
        _ => which_whisper(),
    };

    if whisper_bin.is_none() {
        let msg = "Transcription unavailable — whisper not installed".to_string();
        let _ = app_handle.emit("transcription_complete", TranscriptionCompletePayload {
            entry_id: entry_id.clone(),
            transcript: msg.clone(),
        });
        return Ok(msg);
    }
    let whisper_bin = whisper_bin.unwrap();

    let bundled_model = app_handle
        .path()
        .resource_dir()
        .ok()
        .map(|p| p.join("models").join("ggml-tiny.en.bin"));

    let model_path: PathBuf = match bundled_model {
        Some(ref p) if p.exists() => p.clone(),
        _ => dirs_cache_fallback(),
    };

    let audio = PathBuf::from(&audio_path);
    let output_dir = audio
        .parent()
        .ok_or_else(|| "Cannot determine audio parent directory".to_string())?;
    let stem = audio
        .file_stem()
        .ok_or_else(|| "Cannot determine audio file stem".to_string())?;
    let output_base = output_dir.join(stem);
    let output_json = output_dir.join(format!("{}.json", stem.to_string_lossy()));

    let status = Command::new(&whisper_bin)
        .args([
            "-m", model_path.to_str().unwrap_or_default(),
            "-f", &audio_path,
            "-of", output_base.to_str().unwrap_or_default(),
            "--output-format", "json",
        ])
        .status()
        .map_err(|e| format!("Failed to launch whisper: {e}"))?;

    if !status.success() {
        let msg = format!("whisper exited with status: {status}");
        let _ = app_handle.emit("transcription_complete", TranscriptionCompletePayload {
            entry_id: entry_id.clone(),
            transcript: msg.clone(),
        });
        return Err(msg);
    }

    let json_bytes = fs::read(&output_json)
        .map_err(|e| format!("Cannot read whisper output: {e}"))?;
    let whisper_out: WhisperOutput = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("Cannot parse whisper JSON: {e}"))?;

    let transcript = whisper_out
        .transcription
        .iter()
        .map(|s| s.text.trim())
        .collect::<Vec<_>>()
        .join(" ");

    let _ = app_handle.emit("transcription_complete", TranscriptionCompletePayload {
        entry_id: entry_id.clone(),
        transcript: transcript.clone(),
    });

    let _ = fs::remove_file(&output_json);

    Ok(transcript)
}

fn which_whisper() -> Option<PathBuf> {
    let output = Command::new("which").arg("whisper").output().ok()?;
    if output.status.success() {
        let path_str = String::from_utf8(output.stdout).ok()?.trim().to_string();
        if path_str.is_empty() { None } else { Some(PathBuf::from(path_str)) }
    } else {
        None
    }
}

fn dirs_cache_fallback() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".cache").join("whisper").join("ggml-tiny.en.bin")
}

// ────────────────────────────────────────────────────────────────────────────
// New commands: get_entries, start_recording, stop_recording, save_entry,
//               get_summary
// ────────────────────────────────────────────────────────────────────────────

fn yv_home() -> Result<PathBuf, String> {
    dirs::home_dir()
        .ok_or_else(|| "Cannot determine home directory".to_string())
        .map(|h| h.join(".your-voice"))
}

fn open_db() -> Result<rusqlite::Connection, String> {
    let db_path = yv_home()?.join("journal.db");
    if !db_path.exists() {
        return Err("DB not found".to_string());
    }
    rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())
}

fn open_or_create_db() -> Result<rusqlite::Connection, String> {
    let base = yv_home()?;
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let db_path = base.join("journal.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entries (
            id TEXT PRIMARY KEY,
            date TEXT NOT NULL,
            file_path TEXT NOT NULL,
            duration_secs INTEGER NOT NULL,
            transcript TEXT NOT NULL,
            created_at TEXT NOT NULL
        );"
    ).map_err(|e| e.to_string())?;

    // Add title column — silently ignore if it already exists
    let _ = conn.execute_batch("ALTER TABLE entries ADD COLUMN title TEXT NOT NULL DEFAULT '';");

    // Backfill: for rows with no title, set to first 5 words of transcript
    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, transcript FROM entries WHERE trim(title) = '' AND trim(transcript) != '' AND transcript NOT LIKE '[%'"
        ).map_err(|e| e.to_string())?;
        let collected: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        collected
    };
    for (id, transcript) in rows {
        let title: String = transcript
            .split_whitespace()
            .take(5)
            .collect::<Vec<_>>()
            .join(" ");
        let _ = conn.execute(
            "UPDATE entries SET title = ?1 WHERE id = ?2",
            rusqlite::params![title, id],
        );
    }

    Ok(conn)
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct EntryDto {
    pub id: String,
    pub date: String,
    pub file_path: String,
    pub duration_secs: u64,
    pub transcript: String,
    pub created_at: String,
    pub title: String,
}

#[tauri::command]
pub fn get_entries(month: String, year: u32) -> Result<Vec<EntryDto>, String> {
    let month_num = match month.to_lowercase().as_str() {
        "january"   =>  1u32,
        "february"  =>  2,
        "march"     =>  3,
        "april"     =>  4,
        "may"       =>  5,
        "june"      =>  6,
        "july"      =>  7,
        "august"    =>  8,
        "september" =>  9,
        "october"   => 10,
        "november"  => 11,
        "december"  => 12,
        _           => return Err(format!("Unknown month: {month}")),
    };
    let pattern = format!("{year}-{month_num:02}-%");

    let conn = match open_db() {
        Ok(c) => c,
        Err(_) => return Ok(vec![]),
    };

    let mut stmt = conn
        .prepare("SELECT id, date, file_path, duration_secs, transcript, created_at, COALESCE(title, '') FROM entries WHERE date LIKE ?1 ORDER BY created_at ASC")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![pattern], |row| {
            Ok(EntryDto {
                id:            row.get(0)?,
                date:          row.get(1)?,
                file_path:     row.get(2)?,
                duration_secs: row.get::<_, i64>(3)? as u64,
                transcript:    row.get(4)?,
                created_at:    row.get(5)?,
                title:         row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

// ── rename_entry ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn rename_entry(id: String, title: String) -> Result<(), String> {
    let conn = open_db()?;
    conn.execute(
        "UPDATE entries SET title = ?1 WHERE id = ?2",
        rusqlite::params![title, id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// ── generate_title ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn generate_title(transcript: String) -> Result<String, String> {
    if transcript.trim().is_empty() {
        return Ok("Untitled memory".to_string());
    }

    let prompt = format!(
        "Read this voice journal transcript and respond with ONLY a short title (4-7 words, no quotes, no punctuation at the end). \
        The title should feel like a warm personal journal entry title — poetic but grounded. \
        Transcript: {transcript}"
    );

    let payload = serde_json::json!({
        "model": "llama3.2",
        "prompt": prompt,
        "stream": false
    });

    let output = Command::new("curl")
        .args([
            "-s", "-X", "POST",
            "http://localhost:11434/api/generate",
            "-H", "Content-Type: application/json",
            "-d", &payload.to_string(),
        ])
        .output()
        .map_err(|e| format!("Failed to call Ollama: {e}"))?;

    let response: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Cannot parse Ollama response: {e}"))?;

    let title = response["response"]
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Untitled memory".to_string());

    Ok(title)
}

// ── Recording state (static mutex) ──────────────────────────────────────────

struct RecordingState {
    child: std::process::Child,
    output_path: PathBuf,
    mode: String,
    start_time: std::time::Instant,
}

// SAFETY: Child, PathBuf, String, Instant are all Send.
unsafe impl Send for RecordingState {}

static RECORDING: std::sync::Mutex<Option<RecordingState>> = std::sync::Mutex::new(None);

#[tauri::command]
pub fn start_recording(mode: String) -> Result<(), String> {
    let base = yv_home()?;
    let media_dir = base.join("media");
    fs::create_dir_all(&media_dir).map_err(|e| e.to_string())?;

    // Find ffmpeg
    let ffmpeg = {
        let bundled = base.join("bin").join("ffmpeg");
        if bundled.exists() {
            bundled
        } else {
            let out = Command::new("which").arg("ffmpeg").output()
                .map_err(|e| e.to_string())?;
            let path = String::from_utf8(out.stdout).unwrap_or_default().trim().to_string();
            if path.is_empty() { return Err("ffmpeg not found".to_string()); }
            PathBuf::from(path)
        }
    };

    let date_str = Local::now().format("%Y-%m-%d").to_string();
    let uid = Uuid::new_v4();
    let (ext, args): (&str, Vec<&str>) = if mode == "video" {
        ("mp4", vec!["-f", "avfoundation", "-framerate", "30",
                     "-video_size", "1280x720", "-i", "0:0"])
    } else {
        ("wav", vec!["-f", "avfoundation", "-i", ":0",
                     "-ar", "16000", "-ac", "1"])
    };

    let filename = format!("{date_str}_{uid}_raw.{ext}");
    let output_path = media_dir.join(&filename);

    let mut cmd = Command::new(&ffmpeg);
    cmd.args(&args);
    cmd.args(["-y", output_path.to_str().unwrap_or_default()]);
    cmd.stdin(Stdio::piped());

    let child = cmd.spawn().map_err(|e| format!("Failed to spawn ffmpeg: {e}"))?;

    let mut lock = RECORDING.lock().map_err(|e| e.to_string())?;
    *lock = Some(RecordingState {
        child,
        output_path,
        mode,
        start_time: std::time::Instant::now(),
    });
    Ok(())
}

#[derive(serde::Serialize)]
pub struct RecordingResult {
    pub transcript: String,
    pub file_path: String,
    pub duration_secs: u64,
}

#[tauri::command]
pub fn stop_recording() -> Result<RecordingResult, String> {
    let state = {
        let mut lock = RECORDING.lock().map_err(|e| e.to_string())?;
        lock.take().ok_or_else(|| "No recording in progress".to_string())?
    };

    let RecordingState { mut child, output_path, mode, start_time } = state;
    let duration_secs = start_time.elapsed().as_secs();

    // Send 'q' to ffmpeg stdin to stop gracefully
    if let Some(ref mut stdin) = child.stdin {
        let _ = stdin.write_all(b"q\n");
    }
    let _ = child.wait();

    if mode == "video" {
        // Compress raw video
        let compressed = output_path.with_extension("").with_extension("").with_file_name(
            output_path.file_name().unwrap_or_default().to_string_lossy()
                .replace("_raw.", "_final.")
        );
        let compressed = compressed.with_extension("mp4");

        let _status = Command::new("ffmpeg")
            .args(["-i", output_path.to_str().unwrap_or_default(),
                   "-vcodec", "libx264", "-crf", "28",
                   "-acodec", "aac", "-y",
                   compressed.to_str().unwrap_or_default()])
            .status()
            .ok();
        let _ = fs::remove_file(&output_path);

        // Extract audio for transcription
        let tmp_wav = std::env::temp_dir().join(format!("{}.wav", Uuid::new_v4()));
        let _ext_status = Command::new("ffmpeg")
            .args(["-i", compressed.to_str().unwrap_or_default(),
                   "-vn", "-acodec", "pcm_s16le",
                   "-ar", "16000", "-ac", "1", "-y",
                   tmp_wav.to_str().unwrap_or_default()])
            .status()
            .ok();

        let transcript = run_transcribe(&tmp_wav).unwrap_or_default();
        let _ = fs::remove_file(&tmp_wav);

        Ok(RecordingResult {
            transcript,
            file_path: compressed.to_string_lossy().into_owned(),
            duration_secs,
        })
    } else {
        let transcript = run_transcribe(&output_path).unwrap_or_default();
        Ok(RecordingResult {
            transcript,
            file_path: output_path.to_string_lossy().into_owned(),
            duration_secs,
        })
    }
}

fn run_transcribe(audio: &PathBuf) -> Result<String, String> {
    let base = yv_home()?;

    let whisper_bin = {
        let bundled = base.join("bin").join("whisper");
        if bundled.exists() {
            bundled
        } else {
            which_whisper().ok_or_else(|| "whisper not found".to_string())?
        }
    };

    let model_path = {
        let bundled = base.join("models").join("ggml-tiny.en.bin");
        if bundled.exists() {
            bundled
        } else {
            dirs_cache_fallback()
        }
    };

    let output_base = std::env::temp_dir().join(Uuid::new_v4().to_string());
    let output_json = output_base.with_extension("json");

    Command::new(&whisper_bin)
        .args([
            "-m", model_path.to_str().unwrap_or_default(),
            "-f", audio.to_str().unwrap_or_default(),
            "-of", output_base.to_str().unwrap_or_default(),
            "--output-json",
        ])
        .status()
        .map_err(|e| format!("Failed to launch whisper: {e}"))?;

    let json_bytes = fs::read(&output_json)
        .map_err(|e| format!("Cannot read whisper output: {e}"))?;
    let _ = fs::remove_file(&output_json);

    // The whisper --output-json format uses {"transcription":[{"text":...}]}
    let v: serde_json::Value = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("Cannot parse whisper JSON: {e}"))?;

    let transcript = v["transcription"]
        .as_array()
        .map(|segs| {
            segs.iter()
                .filter_map(|s| s["text"].as_str())
                .map(|t| t.trim())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();

    Ok(transcript)
}

#[tauri::command]
pub fn save_entry(
    transcript: String,
    file_path: String,
    duration_secs: u64,
) -> Result<String, String> {
    let conn = open_or_create_db()?;
    let id = Uuid::new_v4().to_string();
    let now = Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let created_at = now.to_rfc3339();

    conn.execute(
        "INSERT INTO entries (id, date, file_path, duration_secs, transcript, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, date, file_path, duration_secs as i64, transcript, created_at],
    ).map_err(|e| e.to_string())?;

    Ok(id)
}

#[tauri::command]
pub fn get_summary(from: String, to: String) -> Result<String, String> {
    let conn = open_db()?;
    let mut stmt = conn
        .prepare("SELECT transcript FROM entries WHERE date >= ?1 AND date <= ?2 ORDER BY created_at ASC")
        .map_err(|e| e.to_string())?;

    let transcripts: Vec<String> = stmt
        .query_map(rusqlite::params![from, to], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;

    if transcripts.is_empty() {
        return Ok("No journal entries found for this period.".to_string());
    }

    let combined = transcripts.join("\n\n---\n\n");
    let prompt = format!(
        "You are reading a person's private voice journal entries from {from} to {to}.\n\
         Entries:\n{combined}\n\n\
         Please reflect on these entries and answer:\n\
         1. What themes or emotions kept appearing?\n\
         2. What moments or details stand out?\n\
         3. What is the overall emotional arc of this period?\n\
         4. What small joys or wins are worth celebrating?\n\
         5. What does this person seem to care most about?\n\
         6. What would be a compassionate observation?\n\
         7. If you could leave one gentle note for this person, what would it say?\n\n\
         Please write a warm, reflective summary."
    );

    let payload = serde_json::json!({
        "model": "llama3.2",
        "prompt": prompt,
        "stream": false
    });

    let output = Command::new("curl")
        .args([
            "-s", "-X", "POST",
            "http://localhost:11434/api/generate",
            "-H", "Content-Type: application/json",
            "-d", &payload.to_string(),
        ])
        .output()
        .map_err(|e| format!("Failed to call Ollama: {e}"))?;

    let response: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Cannot parse Ollama response: {e}"))?;

    response["response"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Unexpected Ollama response format".to_string())
}
