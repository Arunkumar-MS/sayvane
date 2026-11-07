import { useStore, setPolishEnabled } from "../state/store";
import { Pill, Switch, Tooltip } from "../components/ui";
import { sessionStatus, type StatusPill } from "../lib/status";
import { cx } from "../lib/cx";

export function Header({ title }: { title: string }) {
  const session = useStore((s) => s.snapshot!.session);
  const polish = useStore((s) => s.snapshot!.settings.polishEnabled);
  const hasProfile = useStore((s) => s.snapshot!.settings.activeProfileId !== null);
  const engineState = useStore((s) => s.snapshot!.engine.state);
  const status: StatusPill =
    session.phase === "idle" && engineState === "noModel"
      ? { label: "No speech model", tone: "warning", icon: "alert" }
      : session.phase === "idle" && engineState === "loading"
        ? { label: "Loading model", tone: "progress", icon: "spinner" }
        : sessionStatus(session);
  const tip = polish
    ? "On — only the transcript text is sent to your chosen provider for polishing"
    : hasProfile
      ? "Off — transcripts never leave this computer"
      : "Off — transcripts never leave this computer. Turn on to set up a provider.";

  return (
    <header className="header" data-tauri-drag-region>
      <h1 className="page-title" id="page-title">
        {title}
      </h1>
      <div className="header-right">
        <div className={cx("status-chip-wrap", session.phase === "recording" && "is-recording")} role="status" aria-live="polite" aria-label={`Status: ${status.label}`}>
          <Pill pill={status} className="status-chip" />
        </div>
        <Tooltip text={tip} side="bottom" className="tip-end">
          <div className="polish-toggle">
            <Switch checked={polish} onChange={(v) => void setPolishEnabled(v)} label="AI polish" />
          </div>
        </Tooltip>
      </div>
    </header>
  );
}
