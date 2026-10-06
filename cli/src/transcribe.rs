use std::path::PathBuf;
use std::process::Command;

/// Resolve the whisper binary path.
/// Search order:
///   1. ~/.your-voice/bin/whisper
///   2. `which whisper` via PATH
fn find_whisper_binary() -> Option<PathBuf> {
    // 1. User-installed location
    if let Some(home) = dirs::home_dir() {
        let candidate = home.join(".your-voice").join("bin").join("whisper");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // 2. PATH lookup via `which`
    let result = Command::new("which").arg("whisper").output().ok()?;
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

/// Resolve the ggml-tiny.en.bin model path.
/// Search order:
///   1. ~/.your-voice/models/ggml-tiny.en.bin
///   2. ~/.cache/whisper/ggml-tiny.en.bin
fn find_model() -> Option<PathBuf> {
    if let Some(home) = dirs::home_dir() {
        // 1. User-installed location
        let candidate = home
            .join(".your-voice")
            .join("models")
            .join("ggml-tiny.en.bin");
        if candidate.is_file() {
            return Some(candidate);
        }

        // 2. Cache location
        let candidate = home
            .join(".cache")
            .join("whisper")
            .join("ggml-tiny.en.bin");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

/// Transcribe a WAV file using whisper.cpp.
///
/// Returns the transcript text on success.  On non-fatal failures (missing
/// binary, missing model, non-zero exit) returns an `Ok` with a human-readable
/// placeholder so the caller can degrade gracefully.
pub fn transcribe(audio_path: &str) -> anyhow::Result<String> {
    // -- locate binary --
    let whisper_bin = match find_whisper_binary() {
        Some(p) => p,
        None => {
            return Ok(
                "[transcription unavailable — install whisper.cpp and place binary at ~/.your-voice/bin/whisper]"
                    .to_string(),
            );
        }
    };

    // -- locate model --
    let model_path = match find_model() {
        Some(p) => p,
        None => {
            return Ok(
                "[transcription unavailable — download ggml-tiny.en.bin to ~/.your-voice/models/]"
                    .to_string(),
            );
        }
    };

    // -- build a temp output base (whisper appends ".json") --
    let output_base = {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!(
            "your-voice-whisper-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        tmp
    };
    let output_json = output_base.with_extension("json");

    println!("   Transcribing...");

    // -- run whisper --
    // The correct flag is --output-json (or -oj); whisper.cpp does NOT have
    // --output-format.  The binary appends ".json" to the -of base path.
    let output = Command::new(&whisper_bin)
        .args([
            "-m",
            model_path.to_str().unwrap_or_default(),
            "-f",
            audio_path,
            "-of",
            output_base.to_str().unwrap_or_default(),
            "--output-json",
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Ok(format!("[transcription failed: {}]", stderr));
    }

    // -- parse JSON output --
    //    Expected shape: { "transcription": [{ "text": "..." }, ...] }
    let json_str = std::fs::read_to_string(&output_json).unwrap_or_default();

    // Clean up temp file regardless of parse outcome.
    let _ = std::fs::remove_file(&output_json);

    if json_str.is_empty() {
        return Ok("[transcription failed: whisper produced no output]".to_string());
    }

    let transcript = parse_transcript(&json_str);
    Ok(transcript)
}

/// Parse the whisper JSON and join all segment texts.
/// Expected shape: { "transcription": [{ "text": "..." }, ...] }
fn parse_transcript(json_str: &str) -> String {
    use serde_json::Value;
    if let Ok(Value::Object(root)) = serde_json::from_str(json_str) {
        if let Some(Value::Array(segments)) = root.get("transcription") {
            let text: Vec<String> = segments
                .iter()
                .filter_map(|seg| {
                    seg.get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| s.trim().to_string())
                })
                .filter(|s| !s.is_empty())
                .collect();
            if !text.is_empty() {
                return text.join(" ");
            }
        }
    }
    "[transcription failed: could not parse whisper JSON]".to_string()
}

/// Minimal manual JSON parser — kept for unit tests and as a fallback.
fn parse_transcript_manual(json_str: &str) -> String {
    // Locate the "transcription" array.
    let Some(start) = json_str.find("\"transcription\"") else {
        return "[transcription failed: could not parse whisper JSON]".to_string();
    };

    let body = &json_str[start..];

    // Collect every "text": "..." value within that region.
    let mut segments: Vec<String> = Vec::new();
    let mut search = body;

    while let Some(pos) = search.find("\"text\"") {
        search = &search[pos + 6..]; // skip past "text"

        // Skip whitespace and the colon
        let rest = search.trim_start();
        let rest = rest.trim_start_matches(':').trim_start();

        if !rest.starts_with('"') {
            continue;
        }

        // Extract the quoted string (handles basic \" escapes).
        let inner = &rest[1..]; // skip opening quote
        let mut chars = inner.char_indices();
        let mut value = String::new();
        let mut closed = false;

        while let Some((_, ch)) = chars.next() {
            match ch {
                '\\' => {
                    if let Some((_, next)) = chars.next() {
                        match next {
                            '"' => value.push('"'),
                            'n' => value.push('\n'),
                            't' => value.push('\t'),
                            '\\' => value.push('\\'),
                            other => {
                                value.push('\\');
                                value.push(other);
                            }
                        }
                    }
                }
                '"' => {
                    closed = true;
                    break;
                }
                other => value.push(other),
            }
        }

        if closed {
            let trimmed = value.trim().to_string();
            if !trimmed.is_empty() {
                segments.push(trimmed);
            }
        }
    }

    if segments.is_empty() {
        "[transcription failed: could not parse whisper JSON]".to_string()
    } else {
        segments.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manual_single_segment() {
        let json = r#"{"transcription":[{"text":" Hello world"}]}"#;
        let result = parse_transcript_manual(json);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_parse_manual_multiple_segments() {
        let json = r#"{"transcription":[{"text":" Hello"},{"text":" world"}]}"#;
        let result = parse_transcript_manual(json);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_parse_manual_empty_transcription() {
        let json = r#"{"transcription":[]}"#;
        let result = parse_transcript_manual(json);
        assert!(result.starts_with("[transcription failed"));
    }

    #[test]
    fn test_parse_manual_no_transcription_key() {
        let json = r#"{"other": "data"}"#;
        let result = parse_transcript_manual(json);
        assert!(result.starts_with("[transcription failed"));
    }

    #[test]
    fn test_parse_manual_escaped_quotes() {
        let json = r#"{"transcription":[{"text":" Say \"hi\""}]}"#;
        let result = parse_transcript_manual(json);
        assert_eq!(result, r#"Say "hi""#);
    }
}
