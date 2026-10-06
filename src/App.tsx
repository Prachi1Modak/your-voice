import { useEffect, useRef, useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";

// ── Types ────────────────────────────────────────────────────────────────────

type Entry = {
  day: number;
  month: string;
  title: string;
  time: string;
  color: string;
  icon: "wave" | "video";
  rawEntries: DbEntry[];
};

type DbEntry = {
  id: string;
  date: string;        // "YYYY-MM-DD"
  file_path: string;
  duration_secs: number;
  transcript: string;
  created_at: string;
  title: string;
};

type RecordingResult = {
  transcript: string;
  file_path: string;
  duration_secs: number;
};

type CaptureMode = "audio" | "video";

// ── Constants ────────────────────────────────────────────────────────────────

const months = [
  { name: "October",  short: "OCT", year: 2026, index: 9  },
  { name: "November", short: "NOV", year: 2026, index: 10 },
  { name: "December", short: "DEC", year: 2026, index: 11 },
] as const;

type MonthConfig = (typeof months)[number];

const MONTH_NAME_TO_NUMBER: Record<string, number> = {
  January: 1, February: 2, March: 3, April: 4,
  May: 5, June: 6, July: 7, August: 8,
  September: 9, October: 10, November: 11, December: 12,
};

const COLORS = [
  "#d8e6ca", "#f3d4ab", "#e3cad7", "#c9dde2",
  "#e8d89e", "#d6cfeb", "#efd0c8",
];


// ── Helpers ──────────────────────────────────────────────────────────────────

function formatDuration(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

function dbEntriesToEntries(dbEntries: DbEntry[]): Entry[] {
  // Group by "YYYY-MM-DD" date
  const grouped = new Map<string, DbEntry[]>();
  for (const e of dbEntries) {
    const list = grouped.get(e.date) ?? [];
    list.push(e);
    grouped.set(e.date, list);
  }

  return Array.from(grouped.entries()).map(([date, rows], i) => {
    const [, mm, dd] = date.split("-").map(Number);
    const monthName =
      Object.keys(MONTH_NAME_TO_NUMBER).find(
        (k) => MONTH_NAME_TO_NUMBER[k] === mm,
      ) ?? "Unknown";
    // Use the latest entry for the sticker display
    const latest = rows[rows.length - 1];
    const isVideo = latest.file_path.endsWith(".mp4");
    const title = latest.title?.trim() || latest.transcript.trim().slice(0, 22) || "Memory";
    return {
      day: dd,
      month: monthName,
      title,
      time: formatDuration(latest.duration_secs),
      color: COLORS[i % COLORS.length],
      icon: isVideo ? "video" : "wave",
      rawEntries: rows,
    } satisfies Entry;
  });
}

// ── Tiny SVG icons ────────────────────────────────────────────────────────────

function WaveIcon({ small = false }: { small?: boolean }) {
  return (
    <svg className={small ? "wave-icon small" : "wave-icon"} viewBox="0 0 34 22" aria-hidden="true">
      <path d="M2 11h3m2 0V7m4 9V4m4 15V2m4 14V6m4 7V9m4 2h2" />
    </svg>
  );
}

function VideoIcon() {
  return (
    <svg className="video-icon" viewBox="0 0 24 24" aria-hidden="true">
      <rect x="3" y="6" width="12" height="12" rx="3" />
      <path d="m15 10 6-3v10l-6-3z" />
    </svg>
  );
}

function CloseIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="m6 6 12 12M18 6 6 18" />
    </svg>
  );
}

function Tape({ className = "" }: { className?: string }) {
  return <span className={`tape ${className}`} aria-hidden="true" />;
}

// ── Draggable scrapbook layer ─────────────────────────────────────────────────

function DraggableCanvas() {
  const dragStart = useRef<{
    element: HTMLElement;
    pointerX: number;
    pointerY: number;
    x: number;
    y: number;
  } | null>(null);

  function getDirectChild(target: EventTarget | null, canvas: HTMLDivElement) {
    let element = target instanceof HTMLElement ? target : null;
    while (element && element.parentElement !== canvas) {
      element = element.parentElement;
    }
    return element;
  }

  function startDragging(event: React.PointerEvent<HTMLDivElement>) {
    const element = getDirectChild(event.target, event.currentTarget);
    if (!element) return;
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragStart.current = {
      element,
      pointerX: event.clientX,
      pointerY: event.clientY,
      x: Number(element.dataset.dragX ?? 0),
      y: Number(element.dataset.dragY ?? 0),
    };
    element.classList.add("is-dragging");
  }

  function drag(event: React.PointerEvent<HTMLDivElement>) {
    if (!dragStart.current) return;
    const x = dragStart.current.x + event.clientX - dragStart.current.pointerX;
    const y = dragStart.current.y + event.clientY - dragStart.current.pointerY;
    dragStart.current.element.dataset.dragX = String(x);
    dragStart.current.element.dataset.dragY = String(y);
    dragStart.current.element.style.translate = `${x}px ${y}px`;
  }

  function stopDragging() {
    dragStart.current?.element.classList.remove("is-dragging");
    dragStart.current = null;
  }

  return (
    <div
      className="custom-scrap-layer"
      onPointerDown={startDragging}
      onPointerMove={drag}
      onPointerUp={stopDragging}
      onPointerCancel={stopDragging}
      aria-label="Custom draggable scrapbook layer"
    />
  );
}

// ── Scrapbook title letters ───────────────────────────────────────────────────

function ScrapbookTitle() {
  return (
    <span className="cover-text">
      <span className="letter tile-1">Y</span>
      <span className="letter tile-2">O</span>
      <span className="letter tile-3">U</span>
      <span className="letter tile-4">R</span>
      <span className="voice-row">
        <span className="letter tile-5">V</span>
        <span className="letter tile-6">O</span>
        <span className="letter tile-7">I</span>
        <span className="letter tile-8">C</span>
        <span className="letter tile-9">E</span>
      </span>
    </span>
  );
}

// ── Book opening/closing transition overlay ───────────────────────────────────

function BookTransition({ direction }: { direction: "opening" | "closing" }) {
  return (
    <div className={`book-transition ${direction}`} aria-hidden="true">
      <div className="transition-journal">
        <div className="transition-right-page" />
        <div className="turning-cover">
          <div className="transition-cover-front">
            <ScrapbookTitle />
            <span className="cover-subtitle">a journal for the things you don&apos;t want to forget</span>
            <span className="stitched-heart">♡</span>
            <span className="elastic-band" />
          </div>
          <div className="transition-cover-inside">
            <span className="inside-pocket" />
          </div>
        </div>
        <div className="transition-spine" />
      </div>
    </div>
  );
}

// ── Capture modal ─────────────────────────────────────────────────────────────

function CaptureModal({
  day,
  month,
  existingEntries,
  onClose,
  onSaved,
}: {
  day: number;
  month: string;
  existingEntries: DbEntry[];
  onClose: () => void;
  onSaved?: () => void;
}) {
  const [view, setView] = useState<"list" | "entry" | "picker" | "capture">(
    existingEntries.length > 0 ? "list" : "picker",
  );
  const [selectedEntry, setSelectedEntry] = useState<DbEntry | null>(null);
  const [mode, setMode] = useState<CaptureMode | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [permissionError, setPermissionError] = useState("");
  const [saveResult, setSaveResult] = useState<RecordingResult | null>(null);

  // Title editing state
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const [generatingTitle, setGeneratingTitle] = useState(false);
  const titleInputRef = useRef<HTMLInputElement>(null);

  function startEditTitle(entry: DbEntry) {
    setTitleDraft(entry.title?.trim() || entry.transcript.trim().slice(0, 40) || "");
    setEditingTitle(true);
    window.setTimeout(() => titleInputRef.current?.select(), 30);
  }

  async function saveTitle(entry: DbEntry) {
    const trimmed = titleDraft.trim();
    if (!trimmed || trimmed === (entry.title ?? "")) { setEditingTitle(false); return; }
    try {
      await invoke("rename_entry", { id: entry.id, title: trimmed });
      entry.title = trimmed;           // mutate in place so list refreshes without full refetch
      onSaved?.();
    } catch (err) { console.error("rename_entry failed:", err); }
    setEditingTitle(false);
  }

  async function handleGenerateTitle(entry: DbEntry) {
    setGeneratingTitle(true);
    try {
      const title = await invoke<string>("generate_title", { transcript: entry.transcript });
      setTitleDraft(title);
      setEditingTitle(true);
      window.setTimeout(() => titleInputRef.current?.select(), 30);
    } catch (err) { console.error("generate_title failed:", err); }
    setGeneratingTitle(false);
  }

  const previewVideoRef = useRef<HTMLVideoElement>(null);
  const previewStreamRef = useRef<MediaStream | null>(null);

  // Stop camera preview stream whenever capture view is exited
  useEffect(() => {
    if (view !== "capture") {
      previewStreamRef.current?.getTracks().forEach((t) => t.stop());
      previewStreamRef.current = null;
    }
  }, [view]);

  // Elapsed timer while recording
  useEffect(() => {
    if (!recording) return;
    const timer = window.setInterval(() => setElapsed((v) => v + 1), 1000);
    return () => window.clearInterval(timer);
  }, [recording]);

  async function beginCapture(selectedMode: CaptureMode) {
    setMode(selectedMode);
    setView("capture");
    setPermissionError("");
    setSaveResult(null);
    setElapsed(0);

    if (selectedMode === "video") {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
        previewStreamRef.current = stream;
        // attach after the element mounts
        window.setTimeout(() => {
          if (previewVideoRef.current) {
            previewVideoRef.current.srcObject = stream;
            previewVideoRef.current.play().catch(() => {});
          }
        }, 50);
      } catch (err) {
        setPermissionError(`Camera preview unavailable: ${err}`);
      }
    }

    try {
      await invoke("start_recording", { mode: selectedMode });
      setRecording(true);
    } catch (err) {
      setPermissionError(
        `Could not start recording: ${err}. Make sure ffmpeg is installed.`,
      );
    }
  }

  async function stopCapture() {
    setRecording(false);
    previewStreamRef.current?.getTracks().forEach((t) => t.stop());
    previewStreamRef.current = null;
    try {
      const result = await invoke<RecordingResult>("stop_recording");
      setSaveResult(result);
    } catch (err) {
      setPermissionError(`Recording stopped with error: ${err}`);
    }
  }

  async function handleSave() {
    if (!saveResult) {
      onClose();
      return;
    }
    try {
      await invoke("save_entry", {
        transcript: saveResult.transcript,
        filePath: saveResult.file_path,
        durationSecs: saveResult.duration_secs,
      });
      onSaved?.();
    } catch (err) {
      console.error("save_entry failed:", err);
    }
    onClose();
  }

  const time = `${String(Math.floor(elapsed / 60)).padStart(2, "0")}:${String(
    elapsed % 60,
  ).padStart(2, "0")}`;

  const transcriptText = saveResult?.transcript
    || (recording ? "" : "Your words will appear here as you speak...");

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label="New journal entry">
      <div className={`entry-modal ${mode ? "capture-open" : ""}`}>
        <Tape className="modal-tape" />
        <button className="icon-button modal-close" onClick={onClose} aria-label="Close">
          <CloseIcon />
        </button>

        {view === "list" ? (
          <div className="existing-entry-view">
            <p className="eyebrow">From your journal</p>
            <h2 style={{ marginBottom: 4 }}>{month} {day}</h2>
            <p className="modal-date" style={{ marginBottom: 16 }}>{existingEntries.length} {existingEntries.length === 1 ? "memory" : "memories"}</p>
            <div style={{ display: "flex", flexDirection: "column", gap: 8, marginBottom: 16 }}>
              {existingEntries.map((e) => {
                const isVideo = e.file_path.endsWith(".mp4");
                return (
                  <button
                    key={e.id}
                    onClick={() => { setSelectedEntry(e); setView("entry"); setIsPlaying(false); }}
                    style={{
                      display: "flex", alignItems: "center", gap: 10,
                      background: "#faf6f0", border: "1px solid #e0d8cc",
                      borderRadius: 8, padding: "10px 12px", cursor: "pointer",
                      textAlign: "left",
                    }}
                  >
                    <span style={{ flexShrink: 0 }}>{isVideo ? <VideoIcon /> : <WaveIcon small />}</span>
                    <span style={{ flex: 1, fontSize: 12, color: "#39362e", fontFamily: "Georgia, serif", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {(e.title?.trim() || e.transcript.trim().slice(0, 60)) || "Memory"}
                    </span>
                    <span style={{ flexShrink: 0, fontSize: 11, color: "#9a9080" }}>{formatDuration(e.duration_secs)}</span>
                  </button>
                );
              })}
            </div>
            <div className="add-another">
              <span className="stitched-line" />
              <p>Would you like to record a new one?</p>
              <button onClick={() => setView("picker")}>Record another memory</button>
            </div>
            <p className="privacy-note">
              <span className="lock">▣</span> This memory is stored only on this device
            </p>
          </div>
        ) : view === "entry" && selectedEntry ? (
          <div className="existing-entry-view">
            <p className="eyebrow">From your journal</p>
            <div className="existing-entry-heading">
              <div style={{ flex: 1, minWidth: 0 }}>
                {editingTitle ? (
                  <input
                    ref={titleInputRef}
                    className="title-edit-input"
                    value={titleDraft}
                    onChange={(e) => setTitleDraft(e.target.value)}
                    onBlur={() => saveTitle(selectedEntry)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") saveTitle(selectedEntry);
                      if (e.key === "Escape") setEditingTitle(false);
                    }}
                    maxLength={80}
                  />
                ) : (
                  <h2 className="entry-title-display">
                    {selectedEntry.title?.trim() || selectedEntry.transcript.trim().slice(0, 30) || "Memory"}
                    <button
                      className="title-edit-btn"
                      onClick={() => startEditTitle(selectedEntry)}
                      aria-label="Edit title"
                      title="Rename"
                    >✎</button>
                  </h2>
                )}
                <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 2 }}>
                  <p className="modal-date" style={{ margin: 0 }}>{month} {day}, 2026</p>
                  <button
                    className="generate-title-btn"
                    onClick={() => handleGenerateTitle(selectedEntry)}
                    disabled={generatingTitle}
                    title="Generate a title from transcript using AI"
                  >
                    {generatingTitle ? "…" : "✨ AI title"}
                  </button>
                </div>
              </div>
              <span className="entry-kind">
                {selectedEntry.file_path.endsWith(".mp4") ? <VideoIcon /> : <WaveIcon small />}
                {selectedEntry.file_path.endsWith(".mp4") ? "Video" : "Audio"}
              </span>
            </div>

            {selectedEntry.file_path.endsWith(".mp4") ? (
              <video
                key={selectedEntry.id}
                src={convertFileSrc(selectedEntry.file_path)}
                controls
                style={{ width: "100%", borderRadius: 8, background: "#000", marginBottom: 8 }}
              />
            ) : (
              <div className="memory-audio">
                <audio
                  key={selectedEntry.id}
                  src={convertFileSrc(selectedEntry.file_path)}
                  controls
                  onPlay={() => setIsPlaying(true)}
                  onPause={() => setIsPlaying(false)}
                  onEnded={() => setIsPlaying(false)}
                  style={{ width: "100%", marginBottom: 4 }}
                />
                <div className={`memory-waveform ${isPlaying ? "playing" : ""}`}>
                  {Array.from({ length: 34 }, (_, i) => (
                    <i key={i} style={{ height: `${8 + ((i * 13) % 28)}px` }} />
                  ))}
                  <span style={{ width: isPlaying ? "38%" : "0%" }} />
                </div>
                <time>{formatDuration(selectedEntry.duration_secs)}</time>
              </div>
            )}

            <div className="older-transcript">
              <div className="transcript-heading">
                <span>Transcript</span>
                <em>saved on this device</em>
              </div>
              <p>{selectedEntry.transcript || "No transcript available."}</p>
            </div>

            <div className="add-another">
              <span className="stitched-line" />
              {existingEntries.length > 1 && (
                <button onClick={() => setView("list")} style={{ marginBottom: 8, background: "none", border: "none", color: "#7a7060", fontSize: 12, cursor: "pointer", fontFamily: "Georgia, serif", fontStyle: "italic" }}>
                  ← back to all memories
                </button>
              )}
              <p>Would you like to record a new one?</p>
              <button onClick={() => setView("picker")}>Record another memory</button>
            </div>
            <p className="privacy-note">
              <span className="lock">▣</span> This memory is stored only on this device
            </p>
          </div>
        ) : view === "picker" ? (
          <div className="mode-picker">
            <p className="eyebrow">{existingEntries.length > 0 ? "Another memory" : "A new page"}</p>
            <h2>How do you want to<br />remember today?</h2>
            <p className="modal-date">{month} {day}, 2026</p>
            <div className="mode-options">
              <button onClick={() => beginCapture("audio")} className="mode-card audio">
                <span className="mode-icon"><WaveIcon /></span>
                <strong>Record audio</strong>
                <small>Just your voice</small>
              </button>
              <button onClick={() => beginCapture("video")} className="mode-card video">
                <span className="mode-icon"><VideoIcon /></span>
                <strong>Record video</strong>
                <small>A little moment</small>
              </button>
            </div>
            {permissionError && <p className="permission-error">{permissionError}</p>}
            <p className="privacy-note">
              <span className="lock">▣</span> Everything stays on this device
            </p>
          </div>
        ) : (
          <div className="capture-view">
            <div className="capture-top">
              <div>
                <p className="eyebrow">{mode} entry</p>
                <h2>{month} {day}</h2>
              </div>
              <span className={`recording-pill ${recording ? "active" : ""}`}>
                <i /> {recording ? "Recording" : "Saved locally"}
              </span>
            </div>

            {mode === "video" ? (
              <div className="video-preview">
                <video
                  ref={previewVideoRef}
                  autoPlay
                  muted
                  playsInline
                  style={{
                    width: "100%",
                    height: "100%",
                    objectFit: "cover",
                    borderRadius: 4,
                    background: "#000",
                    display: "block",
                  }}
                />
                <span className="preview-corner top-left" />
                <span className="preview-corner bottom-right" />
              </div>
            ) : (
              <div className="audio-visual">
                {Array.from({ length: 31 }, (_, i) => (
                  <i
                    key={i}
                    style={{ height: `${12 + ((i * 17) % 35)}px`, animationDelay: `${i * -0.07}s` }}
                  />
                ))}
              </div>
            )}

            <div className="record-controls">
              <span className="timer">{time}</span>
              {recording ? (
                <button className="stop-button" onClick={stopCapture} aria-label="Stop recording">
                  <span />
                </button>
              ) : (
                <button className="save-button" onClick={handleSave}>Save entry</button>
              )}
              <span className="timer spacer">{time}</span>
            </div>

            {permissionError && <p className="permission-error">{permissionError}</p>}

            <div className="transcript-card">
              <div className="transcript-heading">
                <span>Live transcript</span>
                <em>{recording ? "listening..." : "complete"}</em>
              </div>
              <p>
                {transcriptText}
                {recording && <span className="cursor" />}
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// ── Weekly reflection modal ───────────────────────────────────────────────────

function SummaryModal({ month, onClose }: { month: MonthConfig; year?: number; onClose: () => void }) {
  const [status, setStatus] = useState<"idle" | "loading" | "done" | "error">("idle");
  const [summary, setSummary] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    async function load() {
      setStatus("loading");
      // current week: Monday → today
      const today = new Date();
      const dayOfWeek = today.getDay(); // 0=Sun
      const daysFromMon = (dayOfWeek + 6) % 7;
      const monday = new Date(today);
      monday.setDate(today.getDate() - daysFromMon);
      const fmt = (d: Date) =>
        `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
      try {
        const result = await invoke<string>("get_summary", {
          from: fmt(monday),
          to: fmt(today),
        });
        setSummary(result);
        setStatus("done");
      } catch (err) {
        setError(String(err));
        setStatus("error");
      }
    }
    void load();
  }, []);

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label="Weekly reflection">
      <div className="entry-modal" style={{ maxWidth: 520, maxHeight: "80vh", overflowY: "auto" }}>
        <Tape className="modal-tape" />
        <button className="icon-button modal-close" onClick={onClose} aria-label="Close">
          <CloseIcon />
        </button>
        <p className="eyebrow">your week in</p>
        <h2 style={{ margin: "4px 0 16px" }}>{month.name} {month.year}</h2>

        {status === "loading" && (
          <p style={{ fontStyle: "italic", color: "#7a7060", fontSize: 14 }}>
            Thinking through your week…
          </p>
        )}
        {status === "error" && (
          <p style={{ color: "#b94a3a", fontSize: 13 }}>
            {error.includes("llama") || error.includes("curl") || error.includes("Ollama")
              ? "Ollama isn't running. Start it with: ollama serve"
              : error}
          </p>
        )}
        {status === "done" && (
          <div style={{
            fontSize: 13,
            lineHeight: 1.7,
            color: "#39362e",
            whiteSpace: "pre-wrap",
            fontFamily: "Georgia, serif",
          }}>
            {summary}
          </div>
        )}
      </div>
    </div>
  );
}


function NotebookCover({ onOpen }: { onOpen: () => void }) {
  return (
    <div className="closed-scene">
      <DraggableCanvas />
      <div className="desk-note note-one">remember<br />the little things</div>
      <div className="leaf-sprig" aria-hidden="true">
        <i /><i /><i /><i /><span />
      </div>
      <div className="photo-scrap" aria-hidden="true">
        <div className="sun" /><div className="hills" />
      </div>
      <button className="closed-book" onClick={onOpen} aria-label="Open Your Voice journal">
        <span className="page-block" />
        <span className="book-cover">
          <ScrapbookTitle />
          <span className="cover-subtitle">a journal for the things you don&apos;t want to forget</span>
          <span className="stitched-heart">♡</span>
          <span className="elastic-band" />
        </span>
      </button>
      <p className="open-hint">tap the journal to open</p>
    </div>
  );
}

// ── Month calendar grid ───────────────────────────────────────────────────────

function MonthCalendar({
  month,
  entries,
  onEntry,
  className = "",
}: {
  month: MonthConfig;
  entries: Entry[];
  onEntry: (day: number, month: string) => void;
  className?: string;
}) {
  const startOffset = new Date(month.year, month.index, 1).getDay();
  const numberOfDays = new Date(month.year, month.index + 1, 0).getDate();
  const calendar = [
    ...Array(startOffset).fill(null),
    ...Array.from({ length: numberOfDays }, (_, i) => i + 1),
  ];

  const today = new Date();

  return (
    <div className={`calendar calendar-page-layer ${className}`}>
      <div className="weekdays">
        {["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"].map((day) => (
          <span key={day}>{day}</span>
        ))}
      </div>
      <div className="calendar-grid">
        {calendar.map((day, index) => {
          if (!day) return <div className="day-cell empty" key={`empty-${index}`} />;
          const entry = entries.find((item) => item.day === day && item.month === month.name);
          const isFuture =
            month.year > today.getFullYear() ||
            (month.year === today.getFullYear() && month.index > today.getMonth()) ||
            (month.year === today.getFullYear() && month.index === today.getMonth() && day > today.getDate());
          if (isFuture) {
            return (
              <div className="day-cell future" key={day} aria-hidden="true">
                <span className="day-number">{day}</span>
              </div>
            );
          }
          return (
            <button
              className={`day-cell ${entry ? "has-entry" : ""}`}
              key={day}
              onClick={() => onEntry(day, month.name)}
              aria-label={entry ? `${month.name} ${day}, ${entry.title}` : `${month.name} ${day}, add new entry`}
            >
              <span className="day-number">{day}</span>
              {entry && (
                <span
                  className="entry-sticker"
                  style={{
                    backgroundColor: entry.color,
                    transform: `rotate(${day % 2 ? -2 : 2}deg)`,
                  }}
                >
                  {entry.icon === "wave" ? <WaveIcon small /> : <VideoIcon />}
                  <b>{entry.title}</b>
                  <small>{entry.rawEntries.length > 1 ? `×${entry.rawEntries.length}` : entry.time}</small>
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}

// ── Left-page month summary ───────────────────────────────────────────────────

function MonthSummaryContent({
  month,
  entries,
  onReflect,
}: {
  month: MonthConfig;
  entries: Entry[];
  onReflect?: () => void;
}) {
  const monthEntries = entries.filter((e) => e.month === month.name);
  return (
    <>
      <Tape className="header-tape" />
      <div className="month-heading">
        <div>
          <p className="eyebrow">Your days in</p>
          <h1>{month.name}</h1>
        </div>
        <span className="year">{month.year}</span>
      </div>
      <div className="doodle-flower" aria-hidden="true">✽</div>
      <blockquote>
        &ldquo;We do not remember days,<br />we remember moments.&rdquo;
        <cite>— Cesare Pavese</cite>
      </blockquote>
      <div className="month-stats">
        <div><strong>{monthEntries.length}</strong><span>memories</span></div>
        <div>
          <strong>
            {Math.round(
              monthEntries.reduce((t, e) => {
                const [m, s] = e.time.split(":").map(Number);
                return t + m * 60 + (s || 0);
              }, 0) / 60,
            )}m
          </strong>
          <span>recorded</span>
        </div>
      </div>
      <div className="paper-pocket">
        <span className="pocket-label">this month&apos;s<br />little things</span>
        <div className="ticket">be here now</div>
        <div className="pressed-flower">✤</div>
      </div>
      {onReflect && (
        <button
          onClick={onReflect}
          style={{
            marginTop: 20,
            width: "100%",
            padding: "9px 0",
            background: "transparent",
            border: "1.5px dashed #b8a89a",
            borderRadius: 8,
            fontFamily: "Georgia, serif",
            fontSize: 12,
            fontStyle: "italic",
            color: "#7a7060",
            cursor: "pointer",
          }}
        >
          ✦ weekly reflection
        </button>
      )}
    </>
  );
}

// ── Right-page content (calendar + nav) ──────────────────────────────────────

function MonthRightContent({
  month,
  entries,
  onEntry,
  onClose,
  onMonthChange,
  onReflect,
  staticPage = false,
}: {
  month: MonthConfig;
  entries: Entry[];
  onEntry: (day: number, month: string) => void;
  onClose: () => void;
  onMonthChange: (index: number) => void;
  onReflect?: () => void;
  staticPage?: boolean;
}) {
  return (
    <>
      <div className="page-edge right-edge" />
      <button className="close-book-button" onClick={onClose} tabIndex={staticPage ? -1 : 0}>
        <svg viewBox="0 0 28 28" aria-hidden="true">
          <path d="M5 7h7c3 0 5 2 5 5v10H9c-2 0-4-2-4-4V7Zm12 5c0-3 2-5 5-5h1v11c0 2-2 4-4 4h-2V12Z" />
        </svg>
        Close journal
      </button>
      <MonthCalendar month={month} entries={entries} onEntry={onEntry} />
      <div className="page-footer month-page-footer">
        <span className="footer-whisper">listen to your days · one memory at a time</span>
        {onReflect && (
          <button
            className="reflect-mobile-btn"
            onClick={onReflect}
            tabIndex={staticPage ? -1 : 0}
          >
            ✦ reflect
          </button>
        )}
        <nav className="month-bottom-nav" aria-label="Other journal months">
          {months.map((option, index) =>
            option.name !== month.name ? (
              <button
                key={option.name}
                onClick={() => onMonthChange(index)}
                tabIndex={staticPage ? -1 : 0}
              >
                {option.name}
                <span>›</span>
              </button>
            ) : null,
          )}
        </nav>
      </div>
    </>
  );
}

// ── Open journal spread ───────────────────────────────────────────────────────

function CalendarSpread({
  entries,
  onEntry,
  onClose,
  onMonthChange,
  onReflect,
}: {
  entries: Entry[];
  onEntry: (day: number, month: string) => void;
  onClose: () => void;
  onMonthChange: (monthName: string, year: number) => void;
  onReflect: () => void;
}) {
  const [monthIndex, setMonthIndex] = useState(0);
  const [pendingMonth, setPendingMonth] = useState<number | null>(null);
  const [turnDirection, setTurnDirection] = useState<"forward" | "backward">("forward");
  const activeMonth = months[monthIndex];
  const shownMonth = pendingMonth === null ? activeMonth : months[pendingMonth];

  function turnTo(nextIndex: number) {
    if (nextIndex === monthIndex || pendingMonth !== null) return;
    setTurnDirection(nextIndex > monthIndex ? "forward" : "backward");
    setPendingMonth(nextIndex);
    window.setTimeout(() => {
      setMonthIndex(nextIndex);
      setPendingMonth(null);
      const next = months[nextIndex];
      onMonthChange(next.name, next.year);
    }, 720);
  }

  return (
    <div className="open-scene">
      <div className="book-shadow" />
      <div className="open-book">
        <div className="book-page left-page">
          <div className="page-edge left-edge" />
          <MonthSummaryContent month={activeMonth} entries={entries} onReflect={onReflect} />
        </div>

        <div className="spine" />

        <div className="book-page right-page">
          <MonthRightContent
            month={shownMonth}
            entries={entries}
            onEntry={onEntry}
            onClose={onClose}
            onMonthChange={turnTo}
            onReflect={onReflect}
          />
          {pendingMonth !== null && (
            <div className={`month-turn-leaf ${turnDirection}`}>
              <div className="month-leaf-face month-leaf-front">
                <MonthRightContent
                  month={activeMonth}
                  entries={entries}
                  onEntry={onEntry}
                  onClose={onClose}
                  onMonthChange={turnTo}
                  staticPage
                />
              </div>
              <div className="month-leaf-face month-leaf-back">
                <MonthSummaryContent month={shownMonth} entries={entries} />
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ── Root App ──────────────────────────────────────────────────────────────────

export default function App() {
  const [isOpen, setIsOpen] = useState(false);
  const [transition, setTransition] = useState<"opening" | "closing" | null>(null);
  const [selectedEntry, setSelectedEntry] = useState<{ day: number; month: string } | null>(null);
  const [entries, setEntries] = useState<Entry[]>([]);
  const [showSummary, setShowSummary] = useState(false);
  // Track the currently-shown month so we can refetch after save
  const [activeMonthName, setActiveMonthName] = useState<string>("October");
  const [activeMonthYear, setActiveMonthYear] = useState<number>(2026);

  async function fetchEntries(monthName: string, year: number) {
    try {
      const dbEntries = await invoke<DbEntry[]>("get_entries", { month: monthName, year });
      setEntries(dbEntriesToEntries(dbEntries));
    } catch (err) {
      // DB may not exist yet — that's fine, just show empty calendar
      console.warn("get_entries failed:", err);
      setEntries([]);
    }
  }

  // Fetch entries when the journal is opened
  useEffect(() => {
    if (isOpen) {
      void fetchEntries(activeMonthName, activeMonthYear);
    }
  }, [isOpen, activeMonthName, activeMonthYear]);

  function openBook() {
    setIsOpen(true);
    setTransition("opening");
    window.setTimeout(() => setTransition(null), 1100);
  }

  function closeBook() {
    setTransition("closing");
    window.setTimeout(() => {
      setIsOpen(false);
      setTransition(null);
    }, 1100);
  }

  function handleMonthChange(monthName: string, year: number) {
    setActiveMonthName(monthName);
    setActiveMonthYear(year);
  }

  const existingEntries = selectedEntry
    ? (entries.find(
        (e) => e.day === selectedEntry.day && e.month === selectedEntry.month,
      )?.rawEntries ?? [])
    : [];

  return (
    <main className={`app-shell ${isOpen ? "journal-open" : ""} ${transition ? "is-transitioning" : ""}`}>
      <div className="paper-grain" aria-hidden="true" />
      {!isOpen ? (
        <NotebookCover onOpen={openBook} />
      ) : (
        <CalendarSpread
          entries={entries}
          onEntry={(day, month) => setSelectedEntry({ day, month })}
          onClose={closeBook}
          onMonthChange={handleMonthChange}
          onReflect={() => setShowSummary(true)}
        />
      )}
      {transition && <BookTransition direction={transition} />}
      {selectedEntry && (
        <CaptureModal
          day={selectedEntry.day}
          month={selectedEntry.month}
          existingEntries={existingEntries}
          onClose={() => setSelectedEntry(null)}
          onSaved={() => void fetchEntries(activeMonthName, activeMonthYear)}
        />
      )}
      {showSummary && (
        <SummaryModal
          month={months.find((m) => m.name === activeMonthName) ?? months[0]}
          year={activeMonthYear}
          onClose={() => setShowSummary(false)}
        />
      )}
    </main>
  );
}
