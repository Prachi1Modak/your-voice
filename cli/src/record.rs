use anyhow::{anyhow, Context, Result};
use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use hound::{WavSpec, WavWriter};
use std::fs;
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

// ── Directory helper ──────────────────────────────────────────────────────────

pub(crate) fn media_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME environment variable not set"))?;
    let dir = home.join(".your-voice").join("media");
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create media directory: {}", dir.display()))?;
    Ok(dir)
}

// ── Convenience type alias ────────────────────────────────────────────────────

type SharedWriter = Arc<Mutex<Option<WavWriter<std::io::BufWriter<fs::File>>>>>;

// ── Public API ────────────────────────────────────────────────────────────────

/// Record audio from the default microphone and save to a WAV file.
///
/// Returns `(absolute_file_path, duration_secs)`.
/// Recording stops when the user presses Ctrl+C.
pub fn record_audio() -> Result<(String, u64)> {
    // ── 1. Output path ────────────────────────────────────────────────────────
    let dir = media_dir()?;
    let date_str = Local::now().format("%Y-%m-%d").to_string();
    let file_name = format!("{}_{}.wav", date_str, Uuid::new_v4());
    let file_path = dir.join(&file_name);
    let file_path_str = file_path
        .to_str()
        .ok_or_else(|| anyhow!("File path contains non-UTF-8 characters"))?
        .to_owned();

    // ── 2. cpal device + config ───────────────────────────────────────────────
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("No default input device found"))?;

    let supported = device
        .default_input_config()
        .context("Failed to get default input config")?;

    let channels = supported.channels();
    let sample_rate = supported.sample_rate();
    let sample_format = supported.sample_format();

    // ── 3. WavWriter (streaming write — no in-memory buffer) ─────────────────
    let spec = WavSpec {
        channels,
        sample_rate: sample_rate.0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let wav_writer = WavWriter::create(&file_path, spec)
        .with_context(|| format!("Failed to create WAV file: {}", file_path.display()))?;
    let writer: SharedWriter = Arc::new(Mutex::new(Some(wav_writer)));

    // ── 4. Shared stop flag ───────────────────────────────────────────────────
    let running = Arc::new(AtomicBool::new(true));
    {
        let r = Arc::clone(&running);
        ctrlc::set_handler(move || {
            r.store(false, Ordering::SeqCst);
        })
        .context("Failed to install Ctrl+C handler")?;
    }

    // ── 5. Build cpal input stream ────────────────────────────────────────────
    let stream_config = StreamConfig {
        channels,
        sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };

    let err_fn = |err| eprintln!("Stream error: {}", err);

    let stream = match sample_format {
        SampleFormat::F32 => {
            let w = Arc::clone(&writer);
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    write_f32(data, &w);
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let w = Arc::clone(&writer);
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    write_i16(data, &w);
                },
                err_fn,
                None,
            )
        }
        SampleFormat::U16 => {
            let w = Arc::clone(&writer);
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    write_u16(data, &w);
                },
                err_fn,
                None,
            )
        }
        other => return Err(anyhow!("Unsupported sample format: {:?}", other)),
    }
    .context("Failed to build input stream")?;

    // ── 6. Record until Ctrl+C ────────────────────────────────────────────────
    stream.play().context("Failed to start audio stream")?;

    println!("🎙  Recording... press Ctrl+C to stop");

    let start = Instant::now();
    let mut last_sec: u64 = u64::MAX; // force first print

    while running.load(Ordering::SeqCst) {
        let elapsed = start.elapsed().as_secs();
        if elapsed != last_sec {
            last_sec = elapsed;
            print!("\r  {}s elapsed  ", elapsed);
            let _ = std::io::stdout().flush();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    println!(); // newline after the live counter

    let duration_secs = start.elapsed().as_secs();

    // ── 7. Finalize ───────────────────────────────────────────────────────────
    drop(stream); // stop capturing before touching the writer

    if let Ok(mut guard) = writer.lock() {
        if let Some(w) = guard.take() {
            w.finalize().context("Failed to finalize WAV file")?;
        }
    }

    println!("⏹  Stopped. Saved to: {}", file_path_str);

    Ok((file_path_str, duration_secs))
}

// ── Sample-writing helpers ────────────────────────────────────────────────────

fn write_f32(data: &[f32], writer: &SharedWriter) {
    if let Ok(mut guard) = writer.lock() {
        if let Some(w) = guard.as_mut() {
            for &s in data {
                let sample = (s * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                let _ = w.write_sample(sample);
            }
        }
    }
}

fn write_i16(data: &[i16], writer: &SharedWriter) {
    if let Ok(mut guard) = writer.lock() {
        if let Some(w) = guard.as_mut() {
            for &s in data {
                let _ = w.write_sample(s);
            }
        }
    }
}

fn write_u16(data: &[u16], writer: &SharedWriter) {
    if let Ok(mut guard) = writer.lock() {
        if let Some(w) = guard.as_mut() {
            for &s in data {
                let sample = (s as i32 - 32768) as i16;
                let _ = w.write_sample(sample);
            }
        }
    }
}
