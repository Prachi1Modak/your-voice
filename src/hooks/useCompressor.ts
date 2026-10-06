import { invoke } from '@tauri-apps/api/core';

/**
 * Calls the Rust `compress_media` command via Tauri.
 *
 * @param filePath  - Absolute path to the saved recording.
 * @param mediaType - "audio" or "video".
 * @returns The path of the compressed file, or the original path if
 *          compression fails (so the caller can always proceed).
 */
export async function compressAfterSave(
  filePath: string,
  mediaType: 'video' | 'audio',
): Promise<string> {
  try {
    const compressed = await invoke<string>('compress_media', {
      inputPath: filePath,
      mediaType,
    });
    return compressed;
  } catch (err) {
    console.error('[useCompressor] compress_media failed, using original path:', err);
    return filePath;
  }
}
