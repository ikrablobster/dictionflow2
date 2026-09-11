import { afterEach, expect, it, vi } from "vitest";
import { captureFocusedHotkey, hotkeyFromCode } from "../src/hotkeyCapture";
afterEach(() => vi.useRealTimers());
it("maps both sides of modifiers and rejects typing keys", () => {
  expect(hotkeyFromCode("ControlRight")).toBe("RightCtrl");
  expect(hotkeyFromCode("AltLeft")).toBe("LeftAlt");
  expect(hotkeyFromCode("F12")).toBe("F12");
  expect(hotkeyFromCode("KeyA")).toBeUndefined();
});
it.each(["Escape", "blur", "cancel"])("cleans up capture after %s", async (reason) => {
  const capture = captureFocusedHotkey();
  const check = expect(capture.result).rejects.toThrow("отменено");
  if (reason === "cancel") capture.cancel();
  else if (reason === "blur") window.dispatchEvent(new Event("blur"));
  else window.dispatchEvent(new KeyboardEvent("keydown", { code: reason }));
  await check;
  const event = new KeyboardEvent("keydown", { code: "F2", cancelable: true });
  window.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(false);
});
