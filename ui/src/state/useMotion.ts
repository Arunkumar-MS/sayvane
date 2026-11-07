import { useEffect } from "react";
import { useStore } from "./store";

/** Mirrors settings.reducedMotion onto <html data-motion> so CSS can honor it. */
export function useMotionPreference() {
  const pref = useStore((s) => s.snapshot?.settings.reducedMotion ?? "system");
  useEffect(() => {
    document.documentElement.dataset.motion = pref;
  }, [pref]);
}
