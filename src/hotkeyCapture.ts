export const HOTKEYS = ["RightCtrl", "LeftCtrl", "RightAlt", "LeftAlt", "CapsLock", "RightShift", "LeftShift", ...Array.from({ length: 12 }, (_, i) => `F${i + 1}`)];

export function hotkeyFromCode(code: string): string | undefined {
  const aliases: Record<string, string> = {
    ControlRight: "RightCtrl", ControlLeft: "LeftCtrl", AltRight: "RightAlt", AltLeft: "LeftAlt",
    ShiftRight: "RightShift", ShiftLeft: "LeftShift", CapsLock: "CapsLock",
  };
  return aliases[code] ?? (/^F([1-9]|1[0-2])$/.test(code) ? code : undefined);
}

/** Capture in the focused WebView, without waiting on a global OS hook. */
export function captureFocusedHotkey(timeout = 10_000): { result: Promise<string>; cancel: () => void } {
  let cancel!: () => void;
  const result = new Promise<string>((resolve, reject) => {
    let timer: ReturnType<typeof setTimeout>;
    const finish = (key?: string, error?: string) => {
      clearTimeout(timer);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onBlur);
      if (key) resolve(key); else reject(new Error(error));
    };
    const onBlur = () => finish(undefined, "Назначение отменено: окно потеряло фокус");
    const onKey = (event: KeyboardEvent) => {
      event.preventDefault(); event.stopImmediatePropagation();
      if (event.repeat) return;
      if (event.code === "Escape") { finish(undefined, "Назначение отменено"); return; }
      const key = hotkeyFromCode(event.code);
      if (key) finish(key);
    };
    cancel = () => finish(undefined, "Назначение отменено");
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onBlur);
    timer = setTimeout(() => finish(undefined, "Тайм-аут ожидания клавиши"), timeout);
  });
  return { result, cancel: () => cancel() };
}
