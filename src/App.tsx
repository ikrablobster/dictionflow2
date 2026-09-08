import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Recorder from "./components/Recorder";
import Settings from "./components/Settings";
import History from "./components/History";

type Tab = "main" | "settings" | "history";

export interface EngineStatus {
  state: "idle" | "listening" | "processing" | "error";
  message?: string;
}

export default function App() {
  const [tab, setTab] = useState<Tab>("main");
  const [status, setStatus] = useState<EngineStatus>({ state: "idle" });
  const [lastText, setLastText] = useState<string>("");

  useEffect(() => {
    const unlistenStatus = listen<EngineStatus>("dictation://status", (e) => {
      setStatus(e.payload);
    });
    const unlistenText = listen<string>("dictation://text", (e) => {
      setLastText(e.payload);
    });
    // Проверяем, готово ли ядро (модель Whisper загружена и т.п.)
    invoke("engine_ping").catch(() => {
      setStatus({ state: "error", message: "Ядро распознавания не отвечает" });
    });
    return () => {
      unlistenStatus.then((f) => f());
      unlistenText.then((f) => f());
    };
  }, []);

  return (
    <div className="app-shell">
      <nav className="tabbar">
        <button className={tab === "main" ? "active" : ""} onClick={() => setTab("main")}>
          Диктовка
        </button>
        <button className={tab === "history" ? "active" : ""} onClick={() => setTab("history")}>
          История
        </button>
        <button className={tab === "settings" ? "active" : ""} onClick={() => setTab("settings")}>
          Настройки
        </button>
      </nav>

      <main className="content">
        {tab === "main" && <Recorder status={status} lastText={lastText} />}
        {tab === "history" && <History />}
        {tab === "settings" && <Settings />}
      </main>
    </div>
  );
}
