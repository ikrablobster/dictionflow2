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

  const load = async (q: string) => {
    const res = await invoke<HistoryEntry[]>("search_history", { query: q });
    setEntries(res);
  };

  useEffect(() => {
    load("");
  }, []);

  useEffect(() => {
    const t = setTimeout(() => load(query), 200);
    return () => clearTimeout(t);
  }, [query]);

  const clearAll = async () => {
    await invoke("clear_history");
    setEntries([]);
  };

  return (
    <div className="history">
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
