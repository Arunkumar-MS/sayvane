// Stand-in for "@tauri-apps/api/event" in `vite --mode mock`.
import { subscribeMock } from "../mock";

export type UnlistenFn = () => void;
export interface Event<T> {
  event: string;
  id: number;
  payload: T;
}

export function listen<T>(event: string, handler: (e: Event<T>) => void): Promise<UnlistenFn> {
  return Promise.resolve(subscribeMock(event, (payload) => handler({ event, id: 0, payload: payload as T })));
}
