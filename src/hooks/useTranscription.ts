import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

interface TranscriptionCompletePayload {
  entry_id: string;
  transcript: string;
}

export function useTranscription(
  onComplete: (entryId: string, transcript: string) => void
): {
  transcribeEntry: (audioPath: string, entryId: string) => Promise<void>;
  transcribing: Set<string>;
} {
  const [transcribing, setTranscribing] = useState<Set<string>>(new Set());
  // Keep a stable ref to onComplete so the event listener never goes stale
  const onCompleteRef = useRef(onComplete);
  onCompleteRef.current = onComplete;

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    listen<TranscriptionCompletePayload>(
      "transcription_complete",
      (event) => {
        const { entry_id, transcript } = event.payload;
        setTranscribing((prev) => {
          const next = new Set(prev);
          next.delete(entry_id);
          return next;
        });
        onCompleteRef.current(entry_id, transcript);
      }
    ).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, []); // run once on mount

  async function transcribeEntry(
    audioPath: string,
    entryId: string
  ): Promise<void> {
    setTranscribing((prev) => new Set(prev).add(entryId));
    try {
      await invoke<string>("transcribe_audio", {
        audioPath,
        entryId,
      });
    } catch (err) {
      // Remove from transcribing set even on error; the Rust side emits the
      // event with an error message so onComplete may still fire.
      setTranscribing((prev) => {
        const next = new Set(prev);
        next.delete(entryId);
        return next;
      });
      console.error("transcribe_audio error:", err);
    }
  }

  return { transcribeEntry, transcribing };
}
