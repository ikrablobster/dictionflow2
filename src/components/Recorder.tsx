import { invoke } from "@tauri-apps/api/core";
import type { EngineStatus } from "../App";
import { useState } from "react";

export default function Recorder({
  status,
  lastText,
}: {
  status: EngineStatus;
  lastText: string;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const toggle = async () => {
    setPending(true);
    setError("");
    try {
    if (status.state === "listening") {
      await invoke("stop_dictation");
    } else {
      await invoke("start_dictation", { insert: false });
    }
    } catch (e) { setError(String(e)); }
    finally { setPending(false); }
  };

  const stateLabel: Record<EngineStatus["state"], string> = {
    idle: "Готово",
    loading: "Подготовка распознавания…",
    listening: "Слушаю...",
    processing: "Распознаю и исправляю...",
    error: "Ошибка",
  };

  return (
    <div className="recorder">
      <div className={`status-dot ${status.state}`} />
      <h2>{stateLabel[status.state]}</h2>
      {status.message && <p className={status.state === "error" ? "error-text" : "hint"}>{status.message}</p>}
      {error && error !== status.message && <p className="error-text" role="alert">{error}</p>}

      <button disabled={pending || status.state === "loading" || status.state === "processing"} className={`mic-btn ${status.state === "listening" ? "on" : ""}`} onClick={toggle}>
        {status.state === "listening" ? "Остановить" : "Начать диктовку"}
      </button>

      <p className="hint">
        Нажмите «Остановить» для итогового текста. При запуске кнопкой результат появится здесь.
        Для вставки в другое приложение удерживайте горячую клавишу из настроек.
        Максимальная длительность записи — 10 минут.
      </p>

      {lastText && (
        <div className="last-result">
          <div className="label">Последний фрагмент</div>
          <div className="text">{lastText}</div>
        </div>
      )}
    </div>
  );
}
