import { invoke } from "@tauri-apps/api/core";
import type { EngineStatus } from "../App";

export default function Recorder({
  status,
  lastText,
}: {
  status: EngineStatus;
  lastText: string;
}) {
  const toggle = async () => {
    if (status.state === "listening") {
      await invoke("stop_dictation");
    } else {
      await invoke("start_dictation");
    }
  };

  const stateLabel: Record<EngineStatus["state"], string> = {
    idle: "Готово",
    listening: "Слушаю...",
    processing: "Распознаю и исправляю...",
    error: "Ошибка",
  };

  return (
    <div className="recorder">
      <div className={`status-dot ${status.state}`} />
      <h2>{stateLabel[status.state]}</h2>
      {status.message && <p className="error-text">{status.message}</p>}

      <button className={`mic-btn ${status.state === "listening" ? "on" : ""}`} onClick={toggle}>
        {status.state === "listening" ? "Остановить" : "Начать диктовку"}
      </button>

      <p className="hint">
        Или используй горячую клавишу, заданную в настройках, из любого приложения —
        текст автоматически появится в активном окне.
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
