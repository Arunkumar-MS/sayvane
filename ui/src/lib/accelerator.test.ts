import { describe, expect, it } from "vitest";
import { acceleratorKeycaps, captureAccelerator, type KeyLike } from "./accelerator";

const key = (code: string, mods: Partial<KeyLike> = {}, k = code): KeyLike => ({
  key: k,
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("captureAccelerator", () => {
  it("builds Tauri accelerators in canonical modifier order", () => {
    expect(captureAccelerator(key("Space", { ctrlKey: true, altKey: true, shiftKey: true }, " "), "windows")).toEqual({
      kind: "done",
      accelerator: "Control+Alt+Shift+Space",
    });
    expect(captureAccelerator(key("KeyV", { ctrlKey: true, metaKey: true }, "v"), "macos")).toEqual({ kind: "done", accelerator: "Control+Command+V" });
    expect(captureAccelerator(key("Digit1", { altKey: true }, "1"), "windows")).toEqual({ kind: "done", accelerator: "Alt+1" });
    expect(captureAccelerator(key("KeyD", { metaKey: true }, "d"), "windows")).toEqual({ kind: "done", accelerator: "Super+D" });
  });
  it("waits while only modifiers are held", () => {
    expect(captureAccelerator(key("ControlLeft", { ctrlKey: true }, "Control"), "macos")).toEqual({ kind: "pending", preview: ["Control"] });
  });
  it("cancels on bare Escape", () => {
    expect(captureAccelerator(key("Escape"), "macos")).toEqual({ kind: "cancel" });
  });
  it("rejects shortcuts that would block typing", () => {
    expect(captureAccelerator(key("KeyA", {}, "a"), "macos").kind).toBe("invalid");
    expect(captureAccelerator(key("KeyA", { shiftKey: true }, "A"), "macos").kind).toBe("invalid");
  });
  it("allows bare function keys", () => {
    expect(captureAccelerator(key("F8"), "windows")).toEqual({ kind: "done", accelerator: "F8" });
  });
});

describe("acceleratorKeycaps", () => {
  it("renders macOS symbols", () => {
    expect(acceleratorKeycaps("Control+Alt+Space", "macos")).toEqual(["⌃", "⌥", "Space"]);
    expect(acceleratorKeycaps("Fn", "macos")).toEqual(["fn"]);
  });
  it("renders Windows labels", () => {
    expect(acceleratorKeycaps("Control+Alt+Space", "windows")).toEqual(["Ctrl", "Alt", "Space"]);
    expect(acceleratorKeycaps("CommandOrControl+Shift+v", "windows")).toEqual(["Ctrl", "Shift", "V"]);
  });
});
