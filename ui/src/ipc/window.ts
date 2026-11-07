import { getCurrentWindow } from "@tauri-apps/api/window";

/** Begin a native window drag (used by the floating bar). Never throws. */
export function startWindowDrag(): void {
  try {
    void getCurrentWindow()
      .startDragging()
      .catch(() => undefined);
  } catch {
    /* not running inside Tauri */
  }
}
