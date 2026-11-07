// Floating recording bar. Lives in its own transparent, non-activating window.
import { useLayoutEffect, useRef, useState, type MouseEvent, type ReactNode } from "react";
import { Check, CircleAlert, CircleCheck, CircleX, Copy, History, Info, LoaderCircle, Mic, Sparkles, Square, X } from "lucide-react";
import { api } from "../ipc/commands";
import type { SessionView } from "../ipc/types";
import { useStore } from "../state/store";
import { useMotionPreference } from "../state/useMotion";
import { useElapsed } from "../state/useElapsed";
import { Waveform } from "../components/Waveform";
import { formatTimer } from "../lib/format";
import { acceleratorKeycaps } from "../lib/accelerator";
import { startWindowDrag } from "../ipc/window";
import { cx } from "../lib/cx";

/** Keep the insertion target focused: bar controls never take focus on click. */
const noFocus = (e: MouseEvent) => e.preventDefault();

function BarButton({ label, onClick, children, tone }: { label: string; onClick: () => void; children: ReactNode; tone?: "finish" | "cancel" | "plain" }) {
  return (
    <button type="button" className={cx("bar-btn", tone && `is-${tone}`)} aria-label={label} title={label} onMouseDown={noFocus} onClick={onClick}>
      {children}
    </button>
  );
}

export function BarApp() {
  useMotionPreference();
  const session = useStore((s) => s.snapshot?.session ?? null);
  const os = useStore((s) => s.snapshot?.platform.os ?? "macos");
  const fn = useStore((s) => s.snapshot?.settings.fnGestureEnabled ?? false);
  const ptt = useStore((s) => s.snapshot?.settings.shortcuts.find((b) => b.action === "pushToTalk")?.accelerator ?? "Control+Alt+Space");
  const forceHover = new URLSearchParams(location.search).get("state") === "hover";

  const onCapsuleMouseDown = (e: MouseEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    startWindowDrag();
  };
  const onContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    void api.showBarMenu().catch(() => undefined);
  };

  const phase = session?.phase ?? "idle";
  const hint = os === "macos" && fn ? "Hold fn" : `Hold ${acceleratorKeycaps(ptt, os).join(os === "macos" ? "" : "+")}`;

  // Report the visible capsule so the transparent rest of the window passes clicks through.
  const hostRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let last = "";
    const report = () => {
      const el = host.querySelector<HTMLElement>(".bar-idle, .bar");
      if (!el) return;
      const r = el.getBoundingClientRect();
      const key = `${Math.round(r.x)},${Math.round(r.y)},${Math.round(r.width)},${Math.round(r.height)}`;
      if (key === last) return;
      last = key;
      void api.setBarHitRegion(r.x - 4, r.y - 4, r.width + 8, r.height + 8).catch(() => undefined);
    };
    report();
    const ro = new ResizeObserver(report);
    host.querySelectorAll(".bar-idle, .bar").forEach((el) => ro.observe(el));
    const t = window.setInterval(report, 250); // catches width animations between states
    return () => {
      ro.disconnect();
      window.clearInterval(t);
    };
  }, [phase]);

  return (
    <div className="bar-host" ref={hostRef}>
      <div className="sr-only" role="status" aria-live="polite">
        {liveText(session)}
      </div>
      {phase === "idle" ? (
        <div className={cx("bar-idle", forceHover && "is-hover")} onMouseDown={onCapsuleMouseDown} onContextMenu={onContextMenu}>
          <div className="bar-nub" aria-hidden />
          <div className="bar capsule-idle">
            <BarButton label="Start dictation" tone="plain" onClick={() => void api.startDictation("handsFree").catch(() => undefined)}>
              <Mic size={13} aria-hidden />
            </BarButton>
            <span className="bar-hint">{hint}</span>
          </div>
        </div>
      ) : (
        <div className={cx("bar", `is-${phase}`)} key={phaseGroup(phase)} onMouseDown={onCapsuleMouseDown} onContextMenu={onContextMenu}>
          <BarContent session={session!} />
        </div>
      )}
    </div>
  );
}

function phaseGroup(phase: SessionView["phase"]): string {
  if (phase === "savingAudio" || phase === "transcribing" || phase === "savingTranscript" || phase === "delivering") return "processing";
  return phase;
}

function liveText(s: SessionView | null): string {
  if (!s) return "";
  switch (s.phase) {
    case "starting":
      return "Opening microphone";
    case "recording":
      return "Recording";
    case "transcribing":
      return "Transcribing locally";
    case "polishing":
      return "Polishing";
    case "result":
      return s.result?.message ?? "";
    default:
      return "";
  }
}

const cancel = () => void api.cancelSession().catch(() => undefined);

function BarContent({ session }: { session: SessionView }) {
  const elapsed = useElapsed(session);
  switch (session.phase) {
    case "starting":
      return (
        <>
          <span className="bar-icon" aria-hidden>
            <LoaderCircle size={13} className="spin" />
          </span>
          <Waveform bars={11} active={false} muted className="bar-wave" />
          <BarButton label="Cancel" tone="cancel" onClick={cancel}>
            <X size={13} strokeWidth={2.25} aria-hidden />
          </BarButton>
        </>
      );
    case "recording":
      return (
        <>
          <BarButton label="Cancel" tone="cancel" onClick={cancel}>
            <X size={13} strokeWidth={2.25} aria-hidden />
          </BarButton>
          <span className="bar-rec-dot" aria-hidden />
          <Waveform bars={11} active className="bar-wave" />
          {session.mode === "handsFree" && (
            <span className="bar-timer" aria-label="Elapsed">
              {formatTimer(elapsed)}
            </span>
          )}
          <BarButton label="Finish" tone="finish" onClick={() => void api.finishDictation().catch(() => undefined)}>
            <Square size={8} fill="currentColor" strokeWidth={0} aria-hidden />
          </BarButton>
        </>
      );
    case "polishing":
      return (
        <>
          <span className="bar-polish-orb" aria-hidden>
            <Sparkles size={9} />
          </span>
          <span className="bar-label is-polish">Polishing</span>
          <BarButton label="Cancel" tone="cancel" onClick={cancel}>
            <X size={13} strokeWidth={2.25} aria-hidden />
          </BarButton>
        </>
      );
    case "result": {
      const r = session.result;
      return r ? <ResultContent result={r} /> : null;
    }
    default: {
      const label = session.phase === "savingAudio" ? "Saving audio" : session.phase === "delivering" ? "Inserting" : session.phase === "savingTranscript" ? "Saving" : "Transcribing locally";
      return (
        <>
          <span className="bar-dots" aria-hidden>
            <span />
            <span />
            <span />
          </span>
          <span className="bar-label is-shimmer">{label}</span>
          {session.phase !== "delivering" && (
            <BarButton label="Cancel" tone="cancel" onClick={cancel}>
              <X size={13} strokeWidth={2.25} aria-hidden />
            </BarButton>
          )}
        </>
      );
    }
  }
}

function ResultContent({ result }: { result: NonNullable<SessionView["result"]> }) {
  const [copied, setCopied] = useState(false);
  const icon =
    result.tone === "success" ? <CircleCheck size={14} /> : result.tone === "info" ? <Info size={14} /> : result.tone === "warning" ? <CircleAlert size={14} /> : <CircleX size={14} />;
  return (
    <>
      <span className={cx("bar-result-icon", `tone-${result.tone}`)} aria-hidden>
        {icon}
      </span>
      <span className="bar-message" title={result.message}>
        {result.message}
      </span>
      {result.canCopy && (
        <BarButton
          label={copied ? "Copied" : "Copy"}
          tone="plain"
          onClick={() => {
            const p = result.dictationId ? api.copyEntry(result.dictationId, "final") : api.copyLast();
            void p.then(() => setCopied(true)).catch(() => undefined);
          }}
        >
          {copied ? <Check size={13} aria-hidden /> : <Copy size={13} aria-hidden />}
        </BarButton>
      )}
      <BarButton label="Open History" tone="plain" onClick={() => void api.openMain("history").catch(() => undefined)}>
        <History size={13} aria-hidden />
      </BarButton>
    </>
  );
}
