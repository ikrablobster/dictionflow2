import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Recorder from "./components/Recorder";
import Settings from "./components/Settings";
import History from "./components/History";

type Tab = "main" | "settings" | "history";
export interface EngineStatus { state: "idle" | "listening" | "processing" | "error"; message?: string; }

export default function App() {
  const [tab, setTab] = useState<Tab>("main");
  const [status, setStatus] = useState<EngineStatus>({ state: "idle" });
  const [lastText, setLastText] = useState("");

  useEffect(() => {
    const a = listen<EngineStatus>("dictation://status", (e) => setStatus(e.payload));
    const b = listen<string>("dictation://text", (e) => setLastText(e.payload));
    const c = listen<string>("dictaflow://navigate", (e) => setTab(e.payload === "history" ? "history" : "settings"));
    invoke("engine_ping").catch(() => setStatus({ state: "error", message: "Ядро распознавания не отвечает" }));
    return () => { a.then((f) => f()); b.then((f) => f()); c.then((f) => f()); };
  }, []);

  return <div className="app-shell">
    <nav className="tabbar">
      <div className="app-logo"><span>D</span><b>DictaFlow</b></div>
      <div className="nav-items">
        <button className={tab === "main" ? "active" : ""} onClick={() => setTab("main")}><i>●</i><span>Диктовка</span></button>
        <button className={tab === "history" ? "active" : ""} onClick={() => setTab("history")}><i>◷</i><span>История</span></button>
        <button className={tab === "settings" ? "active" : ""} onClick={() => setTab("settings")}><i>⚙</i><span>Настройки</span></button>
      </div>
      <div className="sidebar-foot"><span className="online-dot" /> Готов к диктовке</div>
    </nav>
    <main className="content">{tab === "main" && <Recorder status={status} lastText={lastText} />}{tab === "history" && <History />}{tab === "settings" && <Settings />}</main>
  </div>;
}
