import { useEffect, useRef, useState } from "react";
import type { SessionView } from "../ipc/types";

/** Smooth elapsed time: last core value plus local time since it arrived. */
export function useElapsed(session: SessionView): number {
  const base = useRef({ ms: session.elapsedMs ?? 0, at: performance.now() });
  const [, tick] = useState(0);
  useEffect(() => {
    base.current = { ms: session.elapsedMs ?? 0, at: performance.now() };
  }, [session.elapsedMs, session.sessionId]);
  const running = session.phase === "recording";
  useEffect(() => {
    if (!running) return;
    const t = window.setInterval(() => tick((n) => n + 1), 250);
    return () => clearInterval(t);
  }, [running]);
  if (!running) return session.elapsedMs ?? 0;
  return base.current.ms + (performance.now() - base.current.at);
}
