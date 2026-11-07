import { LoaderCircle, Mic, NotebookPen, Square, X, Sparkles } from "lucide-react";
import { api } from "../../ipc/commands";
import { attempt, useStore } from "../../state/store";
import { Button, Keycaps } from "../../components/ui";
import { Waveform } from "../../components/Waveform";
import { useDictationShortcut } from "../../app/Sidebar";
import { formatTimer } from "../../lib/format";
import { PHASE_LABEL, isSessionActive } from "../../lib/status";
import { useElapsed } from "../../state/useElapsed";
import { cx } from "../../lib/cx";

export function RecordHero() {
  const session = useStore((s) => s.snapshot!.session);
  const engine = useStore((s) => s.snapshot!.engine);
  const micDenied = useStore((s) => s.snapshot!.permissions.microphone === "denied");
  const elapsed = useElapsed(session);

  const phase = session.phase;
  const recording = phase === "recording";
  const starting = phase === "starting";
  const processing = isSessionActive(session) && !recording && !starting;
  const noModel = engine.state === "noModel";
  const blocked = noModel || micDenied;

  const onPrimary = () => {
    if (recording) void attempt("Couldn't finish", api.finishDictation);
    else if (!isSessionActive(session)) void attempt("Couldn't start recording", () => api.startDictation("note"));
  };

  const isNote = session.mode === "note" || !isSessionActive(session);
  let title = "Record a note";
  let sub = "Speak freely — it's transcribed on this computer.";
  if (starting) {
    title = "Opening microphone…";
    sub = "Get ready to speak.";
  } else if (recording) {
    title = isNote ? "Listening…" : `Dictating into ${session.destinationLabel ?? "your app"}`;
    sub = "Click the button when you're done.";
  } else if (processing) {
    title = `${PHASE_LABEL[phase]}…`;
    sub = phase === "polishing" ? "Your AI provider is tidying the transcript." : "Hang tight, this only takes a moment.";
  } else if (phase === "result" && session.result) {
    title = session.result.message;
    sub = "Find it any time in History.";
  } else if (noModel) {
    sub = "Install a speech model above to start.";
  } else if (micDenied) {
    sub = "Allow microphone access above to start.";
  }

  const label = recording ? "Finish recording" : starting ? "Opening microphone" : processing ? PHASE_LABEL[phase] : "Record a note";

  return (
    <section className="card hero" aria-labelledby="hero-title">
      <div className="hero-record">
        <div className={cx("record-btn-wrap", recording && "is-recording")}>
          <span className="record-halo" aria-hidden />
          <button
            type="button"
            className={cx("record-btn", recording && "is-recording", (processing || starting) && "is-busy")}
            onClick={onPrimary}
            disabled={(blocked && !isSessionActive(session)) || processing || starting}
            aria-label={label}
            aria-describedby="hero-sub"
          >
            {recording ? (
              <Square size={22} fill="currentColor" aria-hidden />
            ) : processing || starting ? (
              phase === "polishing" ? <Sparkles size={26} aria-hidden /> : <LoaderCircle size={26} className="spin" aria-hidden />
            ) : (
              <Mic size={28} aria-hidden />
            )}
          </button>
        </div>
        <div className="hero-copy">
          <div className="hero-dest">
            <NotebookPen size={12} aria-hidden />
            {isNote ? "Saving to History" : `Inserting into ${session.destinationLabel ?? "the active app"}`}
          </div>
          <h2 className="hero-title" id="hero-title" aria-live="polite">
            {title}
          </h2>
          <p className="hero-sub" id="hero-sub">
            {sub}
          </p>
          {(recording || starting) && (
            <div className="hero-live">
              <Waveform bars={20} active={recording} muted={starting} className="hero-wave" />
              {recording && <span className="hero-timer mono" aria-label="Elapsed time">{formatTimer(elapsed)}</span>}
              <Button size="sm" variant="ghost" icon={<X size={13} aria-hidden />} onClick={() => void attempt("Couldn't cancel", api.cancelSession)}>
                Cancel
              </Button>
            </div>
          )}
          {processing && (
            <div className="hero-live">
              <span className={cx("hero-shimmer", phase === "polishing" && "is-polish")} aria-hidden />
              <Button size="sm" variant="ghost" icon={<X size={13} aria-hidden />} onClick={() => void attempt("Couldn't cancel", api.cancelSession)}>
                Cancel
              </Button>
            </div>
          )}
        </div>
      </div>
      <BarIllustration />
    </section>
  );
}

function BarIllustration() {
  const os = useStore((s) => s.snapshot!.platform.os);
  const { verb, accelerator } = useDictationShortcut();
  return (
    <div className="hero-illo" aria-label="Dictate in any app">
      <div className="illo-screen" aria-hidden>
        <div className="illo-window">
          <span className="illo-line w80" />
          <span className="illo-line w60" />
          <span className="illo-line w70 has-caret" />
        </div>
        <div className="illo-bar">
          <span className="illo-bar-dot" />
          <Waveform bars={9} active demo className="illo-wave" />
        </div>
        <div className="illo-dock" />
      </div>
      <div className="illo-caption">
        <span>In any app,</span>
        <span className="illo-keys">
          {verb.toLowerCase()} <Keycaps accelerator={accelerator} os={os} size="sm" />
        </span>
        <span>and speak.</span>
      </div>
    </div>
  );
}
