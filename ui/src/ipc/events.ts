// Subscribe to the single core event channel. In `--mode mock` the Tauri event
// module is aliased to the in-memory mock (see vite.config.ts).
import { listen } from "@tauri-apps/api/event";
import { EVENT_CHANNEL, type EventEnvelope } from "./types";

export function onAppEvent(handler: (envelope: EventEnvelope) => void): Promise<() => void> {
  return listen<EventEnvelope>(EVENT_CHANNEL, (e) => handler(e.payload));
}
