// Shortcut capture and display (unit tested).
import type { Os } from "../ipc/types";

export interface KeyLike {
  key: string;
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

export type CaptureResult =
  | { kind: "pending"; preview: string[] }
  | { kind: "cancel" }
  | { kind: "invalid"; reason: string }
  | { kind: "done"; accelerator: string };

const MODIFIER_CODES = new Set([
  "ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight",
  "MetaLeft", "MetaRight", "OSLeft", "OSRight", "Fn", "CapsLock",
]);

const NAMED: Record<string, string> = {
  Space: "Space",
  Enter: "Enter",
  NumpadEnter: "Enter",
  Tab: "Tab",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backquote: "`",
};

function modifierParts(e: KeyLike, os: Os): string[] {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Control");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push(os === "macos" ? "Command" : "Super");
  return parts;
}

export function keyFromCode(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^Numpad[0-9]$/.test(code)) return `Num${code.slice(6)}`;
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  return NAMED[code] ?? null;
}

/** Turns a keydown into a Tauri accelerator such as "Control+Alt+Shift+Space". */
export function captureAccelerator(e: KeyLike, os: Os): CaptureResult {
  const mods = modifierParts(e, os);
  if (e.code === "Escape" || e.key === "Escape") {
    return mods.length === 0 ? { kind: "cancel" } : { kind: "invalid", reason: "Escape is reserved for canceling." };
  }
  if (MODIFIER_CODES.has(e.code) || ["Control", "Alt", "Shift", "Meta", "OS"].includes(e.key)) {
    return { kind: "pending", preview: mods };
  }
  const key = keyFromCode(e.code);
  if (!key) return { kind: "invalid", reason: "That key can't be used in a shortcut." };
  const isFn = /^F\d+$/.test(key);
  if (mods.length === 0 && !isFn) return { kind: "invalid", reason: "Add at least one modifier, like Control or Alt." };
  if (mods.length === 1 && mods[0] === "Shift" && !isFn) return { kind: "invalid", reason: "Shift alone would block normal typing. Add Control or Alt." };
  return { kind: "done", accelerator: [...mods, key].join("+") };
}

const MAC_SYMBOL: Record<string, string> = {
  control: "⌃",
  ctrl: "⌃",
  alt: "⌥",
  option: "⌥",
  shift: "⇧",
  command: "⌘",
  cmd: "⌘",
  super: "⌘",
  meta: "⌘",
  commandorcontrol: "⌘",
  cmdorctrl: "⌘",
};

const WIN_LABEL: Record<string, string> = {
  control: "Ctrl",
  ctrl: "Ctrl",
  alt: "Alt",
  option: "Alt",
  shift: "Shift",
  command: "Win",
  cmd: "Win",
  super: "Win",
  meta: "Win",
  commandorcontrol: "Ctrl",
  cmdorctrl: "Ctrl",
};

const KEY_DISPLAY: Record<string, string> = {
  up: "↑",
  down: "↓",
  left: "←",
  right: "→",
  escape: "Esc",
  enter: "↵",
  backspace: "⌫",
  fn: "fn",
};

/** Splits an accelerator into display keycaps for the given platform. */
export function acceleratorKeycaps(accelerator: string, os: Os): string[] {
  if (!accelerator) return [];
  return accelerator.split("+").map((raw) => {
    const k = raw.trim();
    const lower = k.toLowerCase();
    const mod = os === "macos" ? MAC_SYMBOL[lower] : WIN_LABEL[lower];
    if (mod) return mod;
    if (KEY_DISPLAY[lower]) return KEY_DISPLAY[lower];
    return k.length === 1 ? k.toUpperCase() : k;
  });
}

/** Human-readable accelerator for screen readers: "Control Alt Space". */
export function acceleratorSpoken(accelerator: string, os: Os): string {
  return accelerator
    .split("+")
    .map((k) => {
      const l = k.toLowerCase();
      if (l === "super" || l === "command" || l === "cmd") return os === "macos" ? "Command" : "Windows";
      if (l === "alt" && os === "macos") return "Option";
      return k;
    })
    .join(" ");
}
