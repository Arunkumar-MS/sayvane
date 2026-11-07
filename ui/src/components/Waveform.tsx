import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { useLevel } from "../state/store";
import { cx } from "../lib/cx";

interface Props {
  bars?: number;
  active: boolean;
  /** Muted: flat, dim bars (e.g. while the microphone opens). */
  muted?: boolean;
  className?: string;
  /** Optional override for previews / illustrations. */
  demo?: boolean;
}

/** Live level meter. Each bar rises fast and decays slowly (CSS transitions per direction). */
export function Waveform({ bars = 14, active, muted, className, demo }: Props) {
  const live = useLevel();
  const [demoLevel, setDemoLevel] = useState(0);
  useEffect(() => {
    if (!demo) return;
    let i = 0;
    const t = window.setInterval(() => {
      i++;
      setDemoLevel(0.15 + Math.abs(Math.sin(i / 2.7)) * Math.abs(Math.sin(i / 9)) * 0.8);
    }, 110);
    return () => clearInterval(t);
  }, [demo]);
  const level = demo ? demoLevel : active && !muted ? live : 0;

  // Bell-shaped weights so the center reacts most, like a voice.
  const weights = useMemo(
    () => Array.from({ length: bars }, (_, i) => 0.6 + 0.4 * Math.exp(-(((i - (bars - 1) / 2) / (bars / 3.2)) ** 2))),
    [bars],
  );
  const prev = useRef<number[]>(Array(bars).fill(0.12));
  const v = Math.min(1, Math.pow(Math.max(0, level), 0.5) * 1.3);
  const heights = weights.map((w, i) => {
    const jitter = 0.72 + ((Math.sin(i * 12.9898 + v * 78.233) * 43758.5453) % 1 + 1) % 1 * 0.5;
    return Math.max(0.12, Math.min(1, v * w * jitter + 0.1));
  });
  const dirs = heights.map((h, i) => (h >= prev.current[i] ? "rise" : "fall"));
  prev.current = heights;

  return (
    <div className={cx("waveform", muted && "is-muted", className)} aria-hidden="true">
      {heights.map((h, i) => (
        <span key={i} className={cx("waveform-bar", dirs[i])} style={{ "--h": h.toFixed(3) } as CSSProperties} />
      ))}
    </div>
  );
}
