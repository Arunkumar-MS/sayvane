// Stand-in for "@tauri-apps/api/core" in `vite --mode mock`.
import { handleMockCommand } from "../mock";

export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return handleMockCommand(cmd, args ?? {}) as Promise<T>;
}
