import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { captureFocusedHotkey, HOTKEYS } from "../hotkeyCapture";

interface AudioDevice { name: string; is_default: boolean; }
interface AppConfig {
  hotkey: string;
  language_mode: "auto" | "ru" | "uk" | "en";
  model_size: "tiny" | "base" | "small" | "medium" | "large-v3";
  input_device: string;
  show_transcription_overlay: boolean;
  live_preview: boolean;
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
  hotkey: "RightCtrl", language_mode: "auto", model_size: "tiny", input_device: "",
  show_transcription_overlay: true, live_preview: false, cloud_enabled: false, cloud_api_key: "",
  auto_punctuation: true, grammar_correction: true, voice_commands: true, custom_dictionary: [],
  insertion_mode: "clipboard_paste", start_with_windows: false, minimize_to_tray: true,
};

export default function Settings() {
  const [cfg, setCfg] = useState<AppConfig>(DEFAULTS);
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [level, setLevel] = useState(0);
  const [testing, setTesting] = useState(false);
  const [dictWord, setDictWord] = useState("");
  const [saved, setSaved] = useState(false);
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const meterTimer = useRef<number | null>(null);
  const cancelCapture = useRef<(() => void) | null>(null);

  const refreshDevices = async () => {
    const found = await invoke<AudioDevice[]>("list_input_devices");
    setDevices(found);
  };

  useEffect(() => {
    invoke<AppConfig>("get_config").then(setCfg).catch((e) => setError(String(e)));
    refreshDevices().catch((e) => setError(String(e)));
    return () => { cancelCapture.current?.(); if (meterTimer.current) window.clearInterval(meterTimer.current); invoke("stop_microphone_test").catch(() => {}); };
  }, []);

  const save = async (next: AppConfig) => {
    setSaving(true); setError("");
    try {
      await invoke("set_config", { config: next });
      setCfg(next);
      setSaved(true); window.setTimeout(() => setSaved(false), 1100);
    } catch (e) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const startTest = async (device = cfg.input_device) => {
    setError("");
    try {
    await invoke("start_microphone_test", { device });
    setTesting(true);
    if (meterTimer.current) window.clearInterval(meterTimer.current);
    meterTimer.current = window.setInterval(async () => {
      try { setLevel(await invoke<number>("get_audio_level")); } catch { setLevel(0); }
    }, 80);
    } catch (e) { setError(String(e)); }
  };

  const stopTest = async () => {
    if (meterTimer.current) window.clearInterval(meterTimer.current);
    meterTimer.current = null; setTesting(false); setLevel(0);
    await invoke("stop_microphone_test").catch((e) => setError(String(e)));
  };

  const chooseDevice = async (device: string) => {
    await stopTest().catch(() => {});
    await save({ ...cfg, input_device: device });
  };

  const captureHotkey = async () => {
    if (cancelCapture.current) { cancelCapture.current(); return; }
    setError("");
    try {
      await invoke("set_hotkey_capture", { active: true });
      const capture = captureFocusedHotkey();
      cancelCapture.current = capture.cancel;
      setCapturing(true);
      await save({ ...cfg, hotkey: await capture.result });
    }
    catch (e) { setError(String(e)); }
    finally {
      cancelCapture.current = null;
      setCapturing(false);
      await invoke("set_hotkey_capture", { active: false }).catch(() => {});
    }
  };

  const addWord = () => {
    if (!dictWord.trim()) return;
    save({ ...cfg, custom_dictionary: [...cfg.custom_dictionary, dictWord.trim()] });
    setDictWord("");
  };

  return (
    <div className="settings modern-settings">
      <div className="settings-title"><div><h2>Настройки</h2><p>Настройте DictaFlow под свой голос и рабочий процесс.</p></div><div className="brand-pill">DictaFlow</div></div>
      {error && <p className="error-text" role="alert">{error}</p>}
      <fieldset disabled={saving} style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}>

      <section className="settings-card microphone-card">
        <div className="section-heading"><div><span className="section-icon">◉</span><div><h3>Микрофон</h3><p>Выберите устройство и проверьте уровень входного сигнала.</p></div></div><button className="ghost-button" onClick={() => refreshDevices().catch((e) => setError(String(e)))}>Обновить</button></div>
        <select value={cfg.input_device} onChange={(e) => chooseDevice(e.target.value)}>
          <option value="">Системный микрофон по умолчанию</option>
          {devices.map((d) => <option key={d.name} value={d.name}>{d.name}{d.is_default ? " — по умолчанию" : ""}</option>)}
        </select>
        <div className="meter-row"><div className="level-meter"><div className="level-fill" style={{ width: `${Math.max(2, level * 100)}%` }} /></div><span>{Math.round(level * 100)}%</span></div>
        <button className={testing ? "secondary-button active" : "secondary-button"} onClick={() => testing ? stopTest() : startTest()}>{testing ? "Остановить проверку" : "Проверить микрофон"}</button>
      </section>

      <section className="settings-card">
        <div className="section-heading"><div><span className="section-icon">⌨</span><div><h3>Горячая клавиша</h3><p>Удерживайте клавишу, чтобы говорить; отпустите для вставки.</p></div></div></div>
        <div className="row"><select aria-label="Горячая клавиша" disabled={capturing} value={cfg.hotkey} onChange={(e) => save({ ...cfg, hotkey: e.target.value })}>{HOTKEYS.map((key) => <option key={key}>{key}</option>)}</select><button onClick={captureHotkey}>{capturing ? "Отменить назначение" : "Назначить"}</button></div>
        <p className="muted">Выберите клавишу из списка или нажмите «Назначить», затем Ctrl, Alt, Shift, CapsLock или F1–F12. Esc — отмена; ожидание — 10 секунд.</p>
      </section>

      <section className="settings-card two-column-card">
        <div><h3>Язык</h3><select value={cfg.language_mode} onChange={(e) => save({ ...cfg, language_mode: e.target.value as AppConfig["language_mode"] })}><option value="auto">Авто — RU / UK / EN</option><option value="ru">Русский</option><option value="uk">Украинский</option><option value="en">English</option></select></div>
        <div><h3>Whisper</h3><select value={cfg.model_size} onChange={(e) => save({ ...cfg, model_size: e.target.value as AppConfig["model_size"] })}><option value="tiny">Tiny — быстрее</option><option value="base">Base — баланс</option><option value="small">Small — точнее, медленнее</option><option value="medium">Medium — точнее</option><option value="large-v3">Large v3 — максимум</option></select></div>
      </section>

      <section className="settings-card"><h3>Скорость распознавания</h3><p className="muted">На ноутбуках без ускорения GPU начните с Tiny и отключённого предпросмотра. Выберите конкретный язык выше. Small и более крупные модели на слабом процессоре могут обрабатывать короткую запись десятки секунд. Tiny быстрее, но чаще ошибается.</p><button onClick={() => save({ ...cfg, model_size: "tiny", live_preview: false })}>Включить быстрые настройки</button></section>

      <section className="settings-card"><h3>Поведение диктовки</h3>
        <Toggle label="Показывать всплывающее окно транскрипции" checked={cfg.show_transcription_overlay} onChange={(v) => save({ ...cfg, show_transcription_overlay: v })} />
        <Toggle label="Показывать распознаваемый текст во время речи" checked={cfg.live_preview} onChange={(v) => save({ ...cfg, live_preview: v })} />
        <Toggle label="Автоматическая пунктуация" checked={cfg.auto_punctuation} onChange={(v) => save({ ...cfg, auto_punctuation: v })} />
        <Toggle label="Исправление грамматики" checked={cfg.grammar_correction} onChange={(v) => save({ ...cfg, grammar_correction: v })} />
        <Toggle label="Голосовые команды" checked={cfg.voice_commands} onChange={(v) => save({ ...cfg, voice_commands: v })} />
      </section>

      <section className="settings-card"><h3>Вставка текста</h3><select value={cfg.insertion_mode} onChange={(e) => save({ ...cfg, insertion_mode: e.target.value as AppConfig["insertion_mode"] })}><option value="clipboard_paste">Буфер обмена + Ctrl+V — рекомендуется</option><option value="type">Имитация клавиатуры</option></select><p className="muted">Всплывающее окно не забирает фокус, поэтому текст попадает в приложение, где вы начали диктовку.</p></section>

      <section className="settings-card"><h3>Пользовательский словарь</h3><div className="row"><input placeholder="Имя, термин, аббревиатура…" value={dictWord} onChange={(e) => setDictWord(e.target.value)} onKeyDown={(e) => e.key === "Enter" && addWord()} /><button onClick={addWord}>Добавить</button></div><div className="chips">{cfg.custom_dictionary.map((w, i) => <span key={i} className="chip">{w}<button onClick={() => save({ ...cfg, custom_dictionary: cfg.custom_dictionary.filter((_, x) => x !== i) })}>×</button></span>)}</div></section>

      <section className="settings-card"><h3>Система</h3><Toggle label="Запускать вместе с Windows" checked={cfg.start_with_windows} onChange={(v) => save({ ...cfg, start_with_windows: v })} /><Toggle label="Сворачивать в трей вместо закрытия" checked={cfg.minimize_to_tray} onChange={(v) => save({ ...cfg, minimize_to_tray: v })} /></section>
      </fieldset>
      {saved && <div className="saved-toast">Сохранено</div>}
    </div>
  );
}

function Toggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) {
  return <label className="switch-row aqua-switch"><span>{label}</span><input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} /><i /></label>;
}
