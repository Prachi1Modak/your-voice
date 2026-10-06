use anyhow::{anyhow, Context, Result};
use chrono::Local;
use std::fs;
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::thread;
use uuid::Uuid;

// ── Binary finders ────────────────────────────────────────────────────────────

/// Resolve the ffmpeg binary path.
/// Search order:
///   1. ~/.your-voice/bin/ffmpeg
///   2. `which ffmpeg` via PATH
fn find_ffmpeg_binary() -> Option<PathBuf> {
    // 1. User-installed location
    if let Some(home) = dirs::home_dir() {
        let candidate = home.join(".your-voice").join("bin").join("ffmpeg");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // 2. PATH lookup via `which`
    let result = Command::new("which").arg("ffmpeg").output().ok()?;
    if result.status.success() {
        let path_str = String::from_utf8_lossy(&result.stdout).trim().to_string();
        if !path_str.is_empty() {
            let p = PathBuf::from(path_str);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    None
}

// ── Recording ─────────────────────────────────────────────────────────────────

/// Record webcam + mic via ffmpeg (avfoundation) until the running flag is cleared.
/// Returns the duration of the recording in seconds.
fn record_raw(ffmpeg: &PathBuf, raw_path: &PathBuf, running: Arc<AtomicBool>) -> Result<u64> {
    let mut child = Command::new(ffmpeg)
        .args([
            "-f", "avfoundation",
            "-framerate", "30",
            "-video_size", "1280x720",
            "-i", "0:0",
            "-y",
            raw_path
                .to_str()
                .ok_or_else(|| anyhow!("Raw path contains non-UTF-8 characters"))?,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Failed to spawn ffmpeg at {}", ffmpeg.display()))?;

    let start = Instant::now();

    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(100));
    }

    // Send graceful quit signal to ffmpeg
    if let Some(ref mut stdin) = child.stdin {
        let _ = stdin.write_all(b"q\n");
    }

    child.wait().context("Failed to wait for ffmpeg process")?;

    Ok(start.elapsed().as_secs())
}

// ── Compression ───────────────────────────────────────────────────────────────

/// Re-encode the raw video with libx264/aac to reduce file size.
/// Deletes the raw file on success.
fn compress(
    ffmpeg: &PathBuf,
    raw_path: &PathBuf,
    compressed_path: &PathBuf,
) -> Result<()> {
    let status = Command::new(ffmpeg)
        .args([
            "-i",
            raw_path
                .to_str()
                .ok_or_else(|| anyhow!("Raw path contains non-UTF-8 characters"))?,
            "-vcodec",
            "libx264",
            "-crf",
            "28",
            "-acodec",
            "aac",
            "-y",
            compressed_path
                .to_str()
                .ok_or_else(|| anyhow!("Compressed path contains non-UTF-8 characters"))?,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("Failed to spawn ffmpeg for compression at {}", ffmpeg.display()))?;

    if !status.success() {
        return Err(anyhow!(
            "ffmpeg compression failed with exit code: {}",
            status
        ));
    }

    fs::remove_file(raw_path)
        .with_context(|| format!("Failed to delete raw file: {}", raw_path.display()))?;

    Ok(())
}

// ── Audio extraction ──────────────────────────────────────────────────────────

/// Extract the audio track from the video as a mono 16 kHz WAV for whisper.
/// Returns the path to the temporary WAV file.
fn extract_audio(ffmpeg: &PathBuf, video_path: &PathBuf) -> Result<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let tmp_wav = PathBuf::from(format!("/tmp/your-voice-audio-{}.wav", millis));

    let status = Command::new(ffmpeg)
        .args([
            "-i",
            video_path
                .to_str()
                .ok_or_else(|| anyhow!("Video path contains non-UTF-8 characters"))?,
            "-vn",
            "-acodec",
            "pcm_s16le",
            "-ar",
            "16000",
            "-ac",
            "1",
            "-y",
            tmp_wav
                .to_str()
                .ok_or_else(|| anyhow!("Temp WAV path contains non-UTF-8 characters"))?,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| {
            format!(
                "Failed to spawn ffmpeg for audio extraction at {}",
                ffmpeg.display()
            )
        })?;

    if !status.success() {
        return Err(anyhow!(
            "ffmpeg audio extraction failed with exit code: {}",
            status
        ));
    }

    Ok(tmp_wav)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Record a video journal entry.
///
/// Returns `(compressed_video_path, tmp_wav_path, duration_secs)`.
/// Recording stops when the user presses Ctrl+C.
pub fn record_video() -> Result<(String, String, u64)> {
    // 1. Locate ffmpeg
    let ffmpeg = find_ffmpeg_binary().ok_or_else(|| {
        anyhow!(
            "ffmpeg not found — install it and place the binary at \
             ~/.your-voice/bin/ffmpeg or ensure it is on your PATH"
        )
    })?;

    // 2. Build output paths
    let dir = crate::record::media_dir()?;
    let date_str = Local::now().format("%Y-%m-%d").to_string();
    let id = Uuid::new_v4();

    let raw_path = dir.join(format!("{}_{}_{}.mp4", date_str, id, "raw"));
    let compressed_path = dir.join(format!("{}_{}.mp4", date_str, id));

    // 3. Start recording
    println!("📹  Recording video... press Ctrl+C to stop");

    let running = Arc::new(AtomicBool::new(true));
    {
        let r = Arc::clone(&running);
        ctrlc::set_handler(move || {
            r.store(false, Ordering::SeqCst);
        })
        .context("Failed to install Ctrl+C handler")?;
    }

    let duration_secs = record_raw(&ffmpeg, &raw_path, running)?;

    // 4. Compress
    println!("⏹  Stopped. Compressing...");

    compress(&ffmpeg, &raw_path, &compressed_path)?;

    let compressed_path_str = compressed_path
        .to_str()
        .ok_or_else(|| anyhow!("Compressed path contains non-UTF-8 characters"))?
        .to_owned();

    println!("✅  Saved to: {}", compressed_path_str);

    // 5. Extract audio for transcription
    let tmp_wav = extract_audio(&ffmpeg, &compressed_path)?;

    let tmp_wav_str = tmp_wav
        .to_str()
        .ok_or_else(|| anyhow!("Temp WAV path contains non-UTF-8 characters"))?
        .to_owned();

    Ok((compressed_path_str, tmp_wav_str, duration_secs))
}
