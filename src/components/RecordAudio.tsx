import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Props = {
  onBack: () => void;
};

export default function RecordAudio({ onBack }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const animFrameRef = useRef<number>(0);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const secondsRef = useRef(0);

  const [recording, setRecording] = useState(false);
  const [seconds, setSeconds] = useState(0);
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [micReady, setMicReady] = useState(false);
  const [micError, setMicError] = useState<string | null>(null);

  useEffect(() => {
    let localStream: MediaStream;
    if (!navigator.mediaDevices?.getUserMedia) {
      setMicError("Microphone not available in this context.");
      return;
    }
    navigator.mediaDevices
      .getUserMedia({ audio: true, video: false })
      .then((stream) => {
        localStream = stream;
        streamRef.current = stream;
        const ctx = new AudioContext();
        const analyser = ctx.createAnalyser();
        analyser.fftSize = 1024;
        ctx.createMediaStreamSource(stream).connect(analyser);
        audioCtxRef.current = ctx;
        analyserRef.current = analyser;
        setMicReady(true);
        drawWaveform();
      })
      .catch((e) => {
        console.error("Mic access denied:", e);
        setMicError(`Mic access denied: ${e.message ?? e}`);
      });

    return () => {
      cancelAnimationFrame(animFrameRef.current);
      audioCtxRef.current?.close();
      localStream?.getTracks().forEach((t) => t.stop());
    };
  }, []);

  function drawWaveform() {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d")!;
    const bufferLength = analyserRef.current ? analyserRef.current.frequencyBinCount : 512;
    const dataArray = new Uint8Array(bufferLength);

    function draw() {
      animFrameRef.current = requestAnimationFrame(draw);
      ctx.fillStyle = "#111";
      ctx.fillRect(0, 0, canvas!.width, canvas!.height);

      if (analyserRef.current) {
        analyserRef.current.getByteTimeDomainData(dataArray);
      } else {
        dataArray.fill(128);
      }

      ctx.lineWidth = 2;
      ctx.strokeStyle = "#3ecf8e";
      ctx.beginPath();
      const sliceWidth = canvas!.width / bufferLength;
      let x = 0;
      for (let i = 0; i < bufferLength; i++) {
        const v = dataArray[i] / 128.0;
        const y = (v * canvas!.height) / 2;
        i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
        x += sliceWidth;
      }
      ctx.lineTo(canvas!.width, canvas!.height / 2);
      ctx.stroke();
    }

    draw();
  }

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
    const stream = streamRef.current;
    if (!stream) {
      setMicError("No mic stream — please allow microphone access and reload.");
      return;
    }
    chunksRef.current = [];
    const mimeType = MediaRecorder.isTypeSupported("audio/webm;codecs=opus")
      ? "audio/webm;codecs=opus"
      : undefined;
    const recorder = new MediaRecorder(stream, mimeType ? { mimeType } : undefined);
    recorder.ondataavailable = (e) => {
      if (e.data.size > 0) chunksRef.current.push(e.data);
    };
    recorder.onstop = async () => {
      const blob = new Blob(chunksRef.current, { type: recorder.mimeType });
      const arrayBuffer = await blob.arrayBuffer();
      const uint8 = new Uint8Array(arrayBuffer);
      try {
        const path = await invoke<string>("save_recording", {
          data: Array.from(uint8),
          ext: "opus",
        });
        setSavedPath(path);
      } catch (e) {
        console.error("save_recording failed:", e);
        setMicError(`Save failed: ${e}`);
      }
    };
    recorder.start(100);
    mediaRecorderRef.current = recorder;
    setRecording(true);
    setSeconds(0);
    secondsRef.current = 0;
    setSavedPath(null);
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

      <canvas ref={canvasRef} width={500} height={120} style={styles.canvas} />

      {micError && <p style={styles.errorText}>{micError}</p>}

      <div style={styles.controls}>
        {recording && <span style={styles.timer}>{formatTime(seconds)}</span>}
        {!recording ? (
          <button onClick={startRecording} style={styles.recordBtn} disabled={!micReady}>
            {micReady ? "⏺ Record" : "Loading mic…"}
          </button>
        ) : (
          <button onClick={stopRecording} style={styles.stopBtn}>⏹ Stop</button>
        )}
      </div>

      {savedPath && <p style={styles.savedText}>Saved: {savedPath}</p>}
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
  canvas: {
    background: "#111",
    borderRadius: "12px",
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
  savedText: {
    marginTop: "1rem",
    color: "#3ecf8e",
    fontSize: "0.9rem",
    fontFamily: "monospace",
  },
  errorText: {
    color: "#f85149",
    fontSize: "0.95rem",
    maxWidth: "480px",
    textAlign: "center",
    lineHeight: "1.5",
    marginTop: "0.75rem",
  },
};
