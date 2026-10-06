import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { createEntry } from "../db";

type Status = 'idle' | 'saving' | 'compressing' | 'saved';

interface ProcessRecordingParams {
  data: Uint8Array;
  ext: 'webm' | 'opus';
  mediaType: 'video' | 'audio';
  durationSecs: number;
}

interface ProcessRecordingResult {
  entryId: string;
  filePath: string;
}

export function usePostRecordFlow(): {
  processRecording: (params: ProcessRecordingParams) => Promise<ProcessRecordingResult>;
  status: Status;
  savedPath: string | null;
} {
  const [status, setStatus] = useState<Status>('idle');
  const [savedPath, setSavedPath] = useState<string | null>(null);

  async function processRecording(params: ProcessRecordingParams): Promise<ProcessRecordingResult> {
    const { data, ext, mediaType, durationSecs } = params;

    setStatus('saving');
    const rawPath = await invoke<string>('save_recording', {
      data: Array.from(data),
      ext,
    });

    setStatus('compressing');
    const compressedPath = await invoke<string>('compress_media', {
      inputPath: rawPath,
      mediaType,
    });

    const entryId = await createEntry({
      type: mediaType,
      filePath: compressedPath,
      durationSecs,
    });

    // Fire and forget — transcription runs in the background
    invoke('transcribe_audio', { audioPath: compressedPath, entryId }).catch(console.error);

    setStatus('saved');
    setSavedPath(compressedPath);

    return { entryId, filePath: compressedPath };
  }

  return { processRecording, status, savedPath };
}
