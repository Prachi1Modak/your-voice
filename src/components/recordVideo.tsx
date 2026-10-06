import { useEffect, useRef, useState } from "react";
import { usePostRecordFlow } from "../hooks/usePostRecordFlow";

type Props = {
  onBack: () => void;
};

export default function RecordVideo({ onBack }: Props) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const secondsRef = useRef(0);

  const [recording, setRecording] = useState(false);
  const [seconds, setSeconds] = useState(0);
  const [camError, setCamError] = useState<string | null>(null);
  const [camReady, setCamReady] = useState(false);
  const { processRecording, status, savedPath } = usePostRecordFlow();

  useEffect(() => {
    let stream: MediaStream;
    if (!navigator.mediaDevices?.getUserMedia) {
      setCamError("Camera not available in this context.");
      return;
    }
    navigator.mediaDevices
      .getUserMedia({ video: true, audio: true })
      .then((s) => {
        stream = s;
        if (videoRef.current) {
          videoRef.current.srcObject = s;
          videoRef.current.play().catch(() => {});
        }
        setCamReady(true);
      })
      .catch((e) => {
        console.error("Camera access denied:", e);
        setCamError(`Camera access denied: ${e.message ?? e}`);
      });
    return () => {
      stream?.getTracks().forEach((t) => t.stop());
    };
  }, []);

  useEffect(() => {
    if (!recording) return;
    secondsRef.current = 0;
    const interval = setInterval(() => {
      secondsRef.current += 1;
      setSeconds((s) => s + 1);
    }, 1000);
    return () => clearInterval(interval);
  }, [recording]);

  function startRecording() {
    const stream = videoRef.current?.srcObject as MediaStream | null;
    if (!stream) {
      setCamError("No camera stream — please allow camera access and reload.");
      return;
    }
    chunksRef.current = [];
    const recorder = new MediaRecorder(stream);
    recorder.ondataavailable = (e) => {
      if (e.data.size > 0) chunksRef.current.push(e.data);
    };
    recorder.onstop = async () => {
      const blob = new Blob(chunksRef.current, { type: "video/webm" });
      const arrayBuffer = await blob.arrayBuffer();
      await processRecording({
        data: new Uint8Array(arrayBuffer),
        ext: "webm",
        mediaType: "video",
        durationSecs: secondsRef.current,
      });
    };
    recorder.start(100); // collect data every 100ms
    mediaRecorderRef.current = recorder;
    setRecording(true);
    setSeconds(0);
    secondsRef.current = 0;
  }

  function stopRecording() {
    mediaRecorderRef.current?.stop();
    setRecording(false);
  }

  function formatTime(s: number) {
    const m = Math.floor(s / 60).toString().padStart(2, "0");
    const sec = (s % 60).toString().padStart(2, "0");
    return `${m}:${sec}`;
  }

  return (
    <div style={styles.container}>
      <button onClick={onBack} style={styles.backButton}>← Back</button>

      {camError ? (
        <p style={styles.errorText}>{camError}</p>
      ) : (
        <video ref={videoRef} autoPlay muted playsInline style={styles.preview} />
      )}

      {!camError && (
        <div style={styles.controls}>
          {recording && <span style={styles.timer}>{formatTime(seconds)}</span>}
          {!recording ? (
            <button onClick={startRecording} style={styles.recordBtn} disabled={!camReady}>
              {camReady ? "⏺ Record" : "Loading camera…"}
            </button>
          ) : (
            <button onClick={stopRecording} style={styles.stopBtn}>⏹ Stop</button>
          )}
        </div>
      )}

      {status === "saving" && <p style={styles.statusText}>Saving…</p>}
      {status === "compressing" && <p style={styles.statusText}>Compressing…</p>}
      {status === "saved" && savedPath && <p style={styles.statusText}>Saved: {savedPath}</p>}
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    height: "100vh",
    background: "#0f0f0f",
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
  },
  backButton: {
    position: "fixed",
    top: "1.5rem",
    left: "1.5rem",
    background: "none",
    border: "1px solid #333",
    color: "#ccc",
    padding: "0.4rem 1rem",
    borderRadius: "8px",
    cursor: "pointer",
    fontSize: "0.9rem",
  },
  preview: {
    width: "640px",
    height: "360px",
    borderRadius: "12px",
    background: "#000",
  },
  controls: {
    marginTop: "1.5rem",
    display: "flex",
    alignItems: "center",
    gap: "1.5rem",
  },
  timer: {
    color: "#fff",
    fontSize: "1.2rem",
    fontFamily: "monospace",
  },
  recordBtn: {
    background: "#e53e3e",
    color: "#fff",
    border: "none",
    padding: "0.7rem 2rem",
    borderRadius: "8px",
    fontSize: "1rem",
    cursor: "pointer",
    opacity: 1,
  },
  stopBtn: {
    background: "#555",
    color: "#fff",
    border: "none",
    padding: "0.7rem 2rem",
    borderRadius: "8px",
    fontSize: "1rem",
    cursor: "pointer",
  },
  statusText: {
    marginTop: "1rem",
    color: "#aaa",
    fontSize: "0.9rem",
    fontFamily: "monospace",
  },
  errorText: {
    color: "#f85149",
    fontSize: "0.95rem",
    maxWidth: "480px",
    textAlign: "center",
    lineHeight: "1.5",
  },
};
