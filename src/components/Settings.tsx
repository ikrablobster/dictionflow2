import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AppConfig {
  hotkey: string;
  language_mode: "auto" | "ru" | "uk" | "en";
  model_size: "tiny" | "base" | "small" | "medium" | "large-v3";
  cloud_enabled: boolean;
  cloud_api_key: string;
  auto_punctuation: boolean;
  grammar_correction: boolean;
  voice_commands: boolean;
  custom_dictionary: string[];
  insertion_mode: "type" | "clipboard_paste";
  start_with_windows: boolean;
  minimize_to_tray: boolean;
}

const DEFAULTS: AppConfig = {
  hotkey: "RightCtrl",
  language_mode: "auto",
  model_size: "small",
  cloud_enabled: false,
  cloud_api_key: "",
  auto_punctuation: true,
  grammar_correction: true,
  voice_commands: true,
  custom_dictionary: [],
  insertion_mode: "type",
  start_with_windows: false,
  minimize_to_tray: true,
};

export default function Settings() {
  const [cfg, setCfg] = useState<AppConfig>(DEFAULTS);
  const [dictWord, setDictWord] = useState("");
  const [saved, setSaved] = useState(false);
  const [capturing, setCapturing] = useState(false);

  useEffect(() => {
    invoke<AppConfig>("get_config").then(setCfg).catch(() => {});
  }, []);

  const save = async (next: AppConfig) => {
    setCfg(next);
    await invoke("set_config", { config: next });
    setSaved(true);
    setTimeout(() => setSaved(false), 1200);
  };

  const captureHotkey = async () => {
    setCapturing(true);
    try {
      const combo = await invoke<string>("capture_next_hotkey");
      await save({ ...cfg, hotkey: combo });
    } finally {
      setCapturing(false);
    }
  };

  const addWord = () => {
    if (!dictWord.trim()) return;
    save({ ...cfg, custom_dictionary: [...cfg.custom_dictionary, dictWord.trim()] });
    setDictWord("");
  };

  return (
    <div className="settings">
      <section>
        <h3>Горячая клавиша</h3>
        <div className="row">
          <input readOnly value={cfg.hotkey} />
          <button onClick={captureHotkey} disabled={capturing}>
            {capturing ? "Нажми комбинацию..." : "Назначить"}
          </button>
        </div>
        <p className="muted">
          Зажми — начинается диктовка, отпусти — текст вставляется в активное окно.
        </p>
      </section>

      <section>
        <h3>Язык распознавания</h3>
        <select
          value={cfg.language_mode}
          onChange={(e) => save({ ...cfg, language_mode: e.target.value as AppConfig["language_mode"] })}
        >
          <option value="auto">Авто (RU / UK / EN микс)</option>
          <option value="ru">Русский</option>
          <option value="uk">Украинский</option>
          <option value="en">Английский</option>
        </select>
      </section>

      <section>
        <h3>Модель распознавания (офлайн, Whisper)</h3>
        <select
          value={cfg.model_size}
          onChange={(e) => save({ ...cfg, model_size: e.target.value as AppConfig["model_size"] })}
        >
          <option value="tiny">tiny — максимально быстрая, ниже точность</option>
          <option value="base">base — баланс скорости</option>
          <option value="small">small — рекомендуется</option>
          <option value="medium">medium — выше точность, медленнее</option>
          <option value="large-v3">large-v3 — максимум точности, нужен GPU/много RAM</option>
        </select>
        <p className="muted">
          Модель скачивается один раз при первом выборе и хранится локально.
        </p>
      </section>

      <section>
        <h3>Обработка текста</h3>
        <label className="switch-row">
          <input
            type="checkbox"
            checked={cfg.auto_punctuation}
            onChange={(e) => save({ ...cfg, auto_punctuation: e.target.checked })}
          />
          Автоматическая пунктуация
        </label>
        <label className="switch-row">
          <input
            type="checkbox"
            checked={cfg.grammar_correction}
            onChange={(e) => save({ ...cfg, grammar_correction: e.target.checked })}
          />
          Исправление грамматики
        </label>
        <label className="switch-row">
          <input
            type="checkbox"
            checked={cfg.voice_commands}
            onChange={(e) => save({ ...cfg, voice_commands: e.target.checked })}
          />
          Голосовые команды («новая строка», «точка», «удали последнее предложение»)
        </label>
      </section>

      <section>
        <h3>Облачный режим (опционально)</h3>
        <label className="switch-row">
          <input
            type="checkbox"
            checked={cfg.cloud_enabled}
            onChange={(e) => save({ ...cfg, cloud_enabled: e.target.checked })}
          />
          Использовать облачный API для повышения точности
        </label>
        {cfg.cloud_enabled && (
          <input
            className="api-key"
            type="password"
            placeholder="API-ключ"
            value={cfg.cloud_api_key}
            onChange={(e) => setCfg({ ...cfg, cloud_api_key: e.target.value })}
            onBlur={() => save(cfg)}
          />
        )}
        <p className="muted">
          По умолчанию приложение работает полностью офлайн. Ключ хранится только локально.
        </p>
      </section>

      <section>
        <h3>Пользовательский словарь</h3>
        <div className="row">
          <input
            placeholder="Имя, термин, аббревиатура..."
            value={dictWord}
            onChange={(e) => setDictWord(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addWord()}
          />
          <button onClick={addWord}>Добавить</button>
        </div>
        <div className="chips">
          {cfg.custom_dictionary.map((w, i) => (
            <span key={i} className="chip">
              {w}
              <button
                onClick={() =>
                  save({
                    ...cfg,
                    custom_dictionary: cfg.custom_dictionary.filter((_, idx) => idx !== i),
                  })
                }
              >
                ×
              </button>
            </span>
          ))}
        </div>
      </section>

      <section>
        <h3>Вставка текста</h3>
        <select
          value={cfg.insertion_mode}
          onChange={(e) => save({ ...cfg, insertion_mode: e.target.value as AppConfig["insertion_mode"] })}
        >
          <option value="type">Имитация набора на клавиатуре (совместимо везде)</option>
          <option value="clipboard_paste">Через буфер обмена + Ctrl+V (быстрее)</option>
        </select>
      </section>

      <section>
        <h3>Системные</h3>
        <label className="switch-row">
          <input
            type="checkbox"
            checked={cfg.start_with_windows}
            onChange={(e) => save({ ...cfg, start_with_windows: e.target.checked })}
          />
          Запускать при старте Windows
        </label>
        <label className="switch-row">
          <input
            type="checkbox"
            checked={cfg.minimize_to_tray}
            onChange={(e) => save({ ...cfg, minimize_to_tray: e.target.checked })}
          />
          Сворачивать в трей вместо закрытия
        </label>
      </section>

      {saved && <div className="saved-toast">Сохранено</div>}
    </div>
  );
}
