import { act } from "react";
import { createRoot, Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Recorder from "../src/components/Recorder";
import Settings from "../src/components/Settings";
import App from "../src/App";

const { invoke, listen, handlers } = vi.hoisted(() => ({
  invoke: vi.fn(), listen: vi.fn(), handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

let container: HTMLDivElement;
let root: Root;
beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  invoke.mockReset(); listen.mockReset(); handlers.clear();
  listen.mockImplementation(async (name, handler) => { handlers.set(name, handler); return () => handlers.delete(name); });
  container = document.createElement("div"); document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => { await act(async () => root.unmount()); container.remove(); });
const render = async (element: React.ReactNode) => { await act(async () => root.render(element)); };
const click = async (button: HTMLButtonElement) => { await act(async () => button.click()); };

describe("recording controls", () => {
  it("prevents duplicate starts while the engine prepares and does not paste into its own UI", async () => {
    let finish!: () => void;
    invoke.mockReturnValue(new Promise<void>((resolve) => { finish = resolve; }));
    await render(<Recorder status={{ state: "idle" }} lastText="" />);
    const button = container.querySelector("button")!;
    await click(button); await click(button);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("start_dictation", { insert: false });
    expect(button.disabled).toBe(true);
    await act(async () => finish());
  });
  it("stops recording and exposes command failures", async () => {
    invoke.mockRejectedValue("Микрофон отключён");
    await render(<Recorder status={{ state: "listening" }} lastText="" />);
    await click(container.querySelector("button")!);
    expect(invoke).toHaveBeenCalledWith("stop_dictation");
    expect(container.querySelector('[role="alert"]')?.textContent).toContain("Микрофон отключён");
  });
  it.each(["loading", "processing"] as const)("disables starting during %s", async (state) => {
    await render(<Recorder status={{ state }} lastText="" />);
    expect(container.querySelector("button")!.disabled).toBe(true);
  });
  it("subscribes to backend status and updates the main button", async () => {
    invoke.mockResolvedValue({ state: "idle" });
    await render(<App />);
    await act(async () => handlers.get("dictation://status")!({ payload: { state: "listening" } }));
    expect(container.querySelector(".mic-btn")?.textContent).toBe("Остановить");
    await act(async () => handlers.get("dictation://status")!({ payload: { state: "error", message: "Ошибка модели" } }));
    expect(container.textContent).toContain("Ошибка модели");
  });
  it("reports event permission failures instead of silently appearing ready", async () => {
    listen.mockRejectedValue(new Error("event.listen not allowed"));
    await render(<App />);
    expect(container.textContent).toContain("event.listen not allowed");
  });
});

describe("hotkey settings", () => {
  it("recovers after a capture timeout and offers a dropdown fallback", async () => {
    vi.useFakeTimers();
    invoke.mockImplementation(async (command) => {
      if (command === "get_config") throw new Error("No saved config");
      if (command === "list_input_devices") return [];
    });
    await render(<Settings />);
    const assign = Array.from(container.querySelectorAll("button")).find((b) => b.textContent === "Назначить")!;
    await click(assign);
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(assign.disabled).toBe(false);
    expect(container.textContent).toContain("Тайм-аут ожидания клавиши");
    expect(container.querySelectorAll('select[aria-label="Горячая клавиша"] option').length).toBe(19);
    expect(invoke).toHaveBeenCalledWith("set_hotkey_capture", { active: false });
    vi.useRealTimers();
  });
  it("assigns the physical right modifier through the button and restores the hook", async () => {
    invoke.mockImplementation(async (command) => {
      if (command === "get_config") throw new Error("No saved config");
      if (command === "list_input_devices") return [];
    });
    await render(<Settings />);
    await click(Array.from(container.querySelectorAll("button")).find((b) => b.textContent === "Назначить")!);
    await act(async () => { window.dispatchEvent(new KeyboardEvent("keydown", { code: "ShiftRight", key: "Shift", bubbles: true })); });
    expect(invoke).toHaveBeenCalledWith("set_config", { config: expect.objectContaining({ hotkey: "RightShift" }) });
    expect(invoke).toHaveBeenLastCalledWith("set_hotkey_capture", { active: false });
    expect(container.textContent).toContain("Назначить");
  });
});
