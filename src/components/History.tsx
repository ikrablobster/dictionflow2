import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface HistoryEntry {
  id: number;
  timestamp: string;
  text: string;
  language: string;
  app_name?: string;
}

export default function History() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let disposed = false;
    const t = setTimeout(() => {
      invoke<HistoryEntry[]>("search_history", { query }).then((rows) => {
        if (!disposed) { setEntries(rows); setError(""); }
      }).catch((e) => { if (!disposed) setError(String(e)); });
    }, 200);
    return () => { disposed = true; clearTimeout(t); };
  }, [query, revision]);

  const clearAll = async () => {
    try { await invoke("clear_history"); setEntries([]); setRevision((v) => v + 1); }
    catch (e) { setError(String(e)); }
  };

  return (
    <div className="history">
      {error && <p className="error-text" role="alert">{error}</p>}
      <div className="row">
        <input
          placeholder="Поиск по истории..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button className="danger" onClick={clearAll}>
          Очистить
        </button>
      </div>

      <ul className="history-list">
        {entries.map((e) => (
          <li key={e.id}>
            <div className="meta">
              <span>{new Date(e.timestamp).toLocaleString("ru-RU")}</span>
              <span className="lang-badge">{e.language}</span>
              {e.app_name && <span className="app-badge">{e.app_name}</span>}
            </div>
            <div className="text">{e.text}</div>
          </li>
        ))}
        {entries.length === 0 && <p className="muted">Пока пусто.</p>}
      </ul>
    </div>
  );
}
