import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

export default function Overlay() {
  const [text, setText] = useState("");
  const [status, setStatus] = useState("Слушаю…");
  const [recording, setRecording] = useState(false);
  const [level, setLevel] = useState(0);

  useEffect(() => {
    const a = listen<string>("dictation://partial", (e) => setText(e.payload));
    const b = listen<{ state: string; message?: string }>("dictation://status", (e) => {
      setRecording(e.payload.state === "listening");
      setStatus(e.payload.message || (e.payload.state === "processing" ? "Обрабатываю…" : e.payload.state === "error" ? "Ошибка" : "Слушаю…"));
    });
    a.catch((e) => setStatus(`Ошибка связи: ${String(e)}`));
    b.catch((e) => setStatus(`Ошибка связи: ${String(e)}`));
    return () => { a.then((f) => f()).catch(() => {}); b.then((f) => f()).catch(() => {}); };
  }, []);

  useEffect(() => {
    if (!recording) { setLevel(0); return; }
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try { const value = await invoke<number>("get_audio_level"); if (!disposed) setLevel(value); }
      catch { if (!disposed) setLevel(0); }
      if (!disposed) timer = setTimeout(poll, 120);
    };
    void poll();
    return () => { disposed = true; clearTimeout(timer); };
  }, [recording]);

  return (
    <div className="voice-overlay">
      <div className="overlay-top">
        <div className="brand-mark">D</div>
        <strong>DictaFlow</strong>
        <span className="record-dot" />
        <span className="overlay-status">{status}</span>
      </div>
      <div className="waveform" aria-hidden="true">
        {Array.from({ length: 34 }, (_, i) => <i key={i} style={{ animation: "none", height: `${3 + level * (12 + 16 * Math.abs(Math.sin(i * 0.7)))}px`, opacity: 0.3 + level * 0.7 }} />)}
      </div>
      <div className={`overlay-text ${text ? "" : "placeholder"}`}>
        {text || "Говорите. Предпросмотр последних 12 секунд появится после обработки; полный текст — после остановки."}
      </div>
    </div>
  );
}
