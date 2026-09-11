import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Recorder from "./components/Recorder";
import Settings from "./components/Settings";
import History from "./components/History";

type Tab = "main" | "settings" | "history";
export interface EngineStatus { state: "idle" | "loading" | "listening" | "processing" | "error"; message?: string; }

export default function App() {
  const [tab, setTab] = useState<Tab>("main");
  const [status, setStatus] = useState<EngineStatus>({ state: "idle" });
  const [lastText, setLastText] = useState("");

  useEffect(() => {
    const a = listen<EngineStatus>("dictation://status", (e) => setStatus(e.payload));
    const b = listen<string>("dictation://text", (e) => setLastText(e.payload));
    const c = listen<string>("dictaflow://navigate", (e) => setTab(e.payload === "history" ? "history" : "settings"));
    let disposed = false;
    Promise.all([a, b, c]).then(async () => {
      const current = await invoke<EngineStatus>("get_engine_status");
      if (!disposed) setStatus(current);
    }).catch((error) => { if (!disposed) setStatus({ state: "error", message: `Ошибка связи с ядром: ${String(error)}` }); });
    return () => { disposed = true; for (const subscription of [a, b, c]) subscription.then((f) => f()).catch(() => {}); };
  }, []);

  return <div className="app-shell">
    <nav className="tabbar">
      <div className="app-logo"><span>D</span><b>DictaFlow</b></div>
      <div className="nav-items">
        <button className={tab === "main" ? "active" : ""} onClick={() => setTab("main")}><i>●</i><span>Диктовка</span></button>
        <button className={tab === "history" ? "active" : ""} onClick={() => setTab("history")}><i>◷</i><span>История</span></button>
        <button className={tab === "settings" ? "active" : ""} onClick={() => setTab("settings")}><i>⚙</i><span>Настройки</span></button>
      </div>
      <div className="sidebar-foot"><span className="online-dot" /> {status.state === "listening" ? "Идёт запись" : status.state === "loading" ? "Загрузка модели" : status.state === "processing" ? "Распознавание" : status.state === "error" ? "Требуется внимание" : "Готов к диктовке"}</div>
    </nav>
    <main className="content">{tab === "main" && <Recorder status={status} lastText={lastText} />}{tab === "history" && <History />}{tab === "settings" && <Settings />}</main>
  </div>;
}
