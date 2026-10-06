import { useState } from "react";
import RecordVideo from "./recordVideo";
import RecordAudio from "./RecordAudio";
type Screen = "home" | "record-video" | "record-audio";

export default function Home() {
const [screen, setScreen] = useState<Screen>("home");

if (screen === "record-video") return (
<RecordVideo onBack={() => setScreen("home")} />
);

if (screen === "record-audio") return <RecordAudio onBack={() => setScreen("home")} />;

return (
    <div style={styles.container}>
    <h1 style={styles.title}>Your Voice</h1>
    <p style={styles.subtitle}>What would you like to record today?</p>
    <div style={styles.cardRow}>
        <div style={styles.card} onClick={() => setScreen("record-video")}>
        <span style={styles.emoji}>🎥</span>
        <span style={styles.label}>Video</span>
        </div>
        <div style={styles.card} onClick={() => setScreen("record-audio")}>
        <span style={styles.emoji}>🎤</span>
        <span style={styles.label}>Audio</span>
        </div>
    </div>
    </div>
);
}

const styles: Record<string, React.CSSProperties> = {
container: {
    height: "100vh",
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    background: "#0f0f0f",
    color: "#ffffff",
    fontFamily: "sans-serif",
},
title: {
    fontSize: "2.5rem",
    marginBottom: "0.5rem",
},
subtitle: {
    fontSize: "1rem",
    color: "#888",
    marginBottom: "3rem",
},
cardRow: {
    display: "flex",
    gap: "2rem",
},
card: {
    width: "180px",
    height: "180px",
    background: "#1a1a1a",
    border: "1px solid #333",
    borderRadius: "16px",
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    cursor: "pointer",
    transition: "border-color 0.2s",
},
emoji: {
    fontSize: "3rem",
    marginBottom: "1rem",
},
label: {
    fontSize: "1.1rem",
    color: "#ccc",
},
backButton: {
position: "fixed" as const,
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
};