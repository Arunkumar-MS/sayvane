import { ArrowRight, Clock, Flame, Gauge, Type } from "lucide-react";
import type { ReactNode } from "react";
import { api } from "../../ipc/commands";
import { navigate, useStore, attempt } from "../../state/store";
import { Button, Callout, Pill } from "../../components/ui";
import { formatCount, formatMinutesSaved, formatTime, formatWpm, greeting, dayLabel } from "../../lib/format";
import { entryStatus } from "../../lib/status";
import { SourceIcon } from "../history/SourceIcon";
import { SetupChecklist } from "./SetupChecklist";
import { RecordHero } from "./RecordHero";

export function HomePage() {
  return (
    <div className="page home">
      <Greeting />
      <StorageBanner />
      <SetupChecklist />
      <RecordHero />
      <StatTiles />
      <RecentEntries />
    </div>
  );
}

function Greeting() {
  const brand = useStore((s) => s.snapshot!.brand.displayName);
  return (
    <div className="greeting">
      <h2 className="greeting-title">{greeting()}</h2>
      <p className="greeting-text">
        Put your cursor in any text field and speak — {brand} transcribes on this computer and types it for you.
      </p>
    </div>
  );
}

export function StorageBanner() {
  const storage = useStore((s) => s.snapshot!.storage);
  const err = useStore((s) => s.storageError);
  const message = err ?? (!storage.ok ? storage.error : null);
  if (!message) return null;
  return (
    <Callout
      tone="error"
      live
      className="storage-banner"
      title="History can't be saved right now"
      action={
        <Button size="sm" variant="secondary" onClick={() => void attempt("Couldn't open the folder", api.revealDataDir)}>
          Show folder
        </Button>
      }
    >
      {message} New recordings may not be kept until this is fixed. Data folder: <span className="mono selectable">{storage.dataDir}</span>
    </Callout>
  );
}

function StatTiles() {
  const stats = useStore((s) => s.stats);
  return (
    <section className="stats" aria-label="Your statistics">
      <StatTile icon={<Type size={15} aria-hidden />} label="Words this week" value={stats ? formatCount(stats.wordsThisWeek) : "—"} />
      <StatTile
        icon={<Gauge size={15} aria-hidden />}
        label="Speaking speed"
        value={stats ? formatWpm(stats.speakingWpm) : "—"}
        unit={stats?.speakingWpm != null ? "wpm" : undefined}
        note={stats?.speakingWpm == null ? "After a few seconds of speech" : undefined}
      />
      <StatTile icon={<Clock size={15} aria-hidden />} label="Time saved" value={stats ? formatMinutesSaved(stats.minutesSaved) : "—"} note="Estimate vs. typing" />
      <StatTile
        icon={<Flame size={15} aria-hidden />}
        label="Streak"
        value={stats ? String(stats.streakDays) : "—"}
        unit={stats ? (stats.streakDays === 1 ? "day" : "days") : undefined}
      />
    </section>
  );
}

function StatTile({ icon, label, value, unit, note }: { icon: ReactNode; label: string; value: string; unit?: string; note?: string }) {
  return (
    <div className="stat">
      <div className="stat-head">
        <span className="stat-icon">{icon}</span>
        <span className="stat-label">{label}</span>
      </div>
      <div className="stat-value">
        {value}
        {unit && <span className="stat-unit">{unit}</span>}
      </div>
      {note && <div className="stat-note">{note}</div>}
    </div>
  );
}

function RecentEntries() {
  const history = useStore((s) => s.history);
  const query = useStore((s) => s.historyQuery);
  const loaded = useStore((s) => s.historyLoaded);
  if (!loaded || query) return null;
  const recent = history.slice(0, 4);
  return (
    <section className="recent" aria-labelledby="recent-title">
      <div className="recent-head">
        <h2 id="recent-title" className="section-title">
          Recent
        </h2>
        {recent.length > 0 && (
          <Button variant="ghost" size="sm" onClick={() => navigate("history")}>
            View all <ArrowRight size={13} aria-hidden />
          </Button>
        )}
      </div>
      {recent.length === 0 ? (
        <div className="recent-empty">Your dictations and notes will appear here.</div>
      ) : (
        <ul className="recent-list">
          {recent.map((e) => {
            const status = entryStatus(e)[0];
            const text = e.finalText.trim() || (e.processingState === "failed" ? "Transcript unavailable — audio saved" : "No transcript");
            const d = new Date(e.createdAtUtc);
            const day = dayLabel(d);
            return (
              <li key={e.id}>
                <button type="button" className="recent-item" onClick={() => navigate("history")}>
                  <span className="recent-icon">
                    <SourceIcon source={e.source} />
                  </span>
                  <span className="recent-body">
                    <span className={e.finalText.trim() ? "recent-text" : "recent-text is-muted"}>{text}</span>
                    <span className="recent-meta">
                      {day === "Today" ? formatTime(e.createdAtUtc) : `${day}, ${formatTime(e.createdAtUtc)}`}
                      {e.destinationAppName && <> · {e.destinationAppName}</>}
                      {e.importName && <> · {e.importName}</>}
                    </span>
                  </span>
                  <Pill pill={status} />
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
