import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

export default function Overlay() {
  const [text, setText] = useState("");
  const [status, setStatus] = useState("Слушаю…");

  useEffect(() => {
    const a = listen<string>("dictation://partial", (e) => setText(e.payload));
    const b = listen<{ state: string }>("dictation://status", (e) => {
      setStatus(e.payload.state === "processing" ? "Обрабатываю…" : "Слушаю…");
    });
    return () => { a.then((f) => f()); b.then((f) => f()); };
  }, []);

  return (
    <div className="voice-overlay">
      <div className="overlay-top">
        <div className="brand-mark">D</div>
        <strong>DictaFlow</strong>
        <span className="record-dot" />
        <span className="overlay-status">{status}</span>
      </div>
      <div className="waveform" aria-hidden="true">
        {Array.from({ length: 34 }, (_, i) => <i key={i} style={{ animationDelay: `${i * 35}ms` }} />)}
      </div>
      <div className={`overlay-text ${text ? "" : "placeholder"}`}>
        {text || "Говорите — текст появится здесь в реальном времени"}
      </div>
    </div>
  );
}
