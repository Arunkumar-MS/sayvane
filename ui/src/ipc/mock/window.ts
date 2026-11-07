// Stand-in for "@tauri-apps/api/window" in `vite --mode mock`.
export function getCurrentWindow() {
  return {
    startDragging: () => Promise.resolve(),
    label: "mock",
  };
}
