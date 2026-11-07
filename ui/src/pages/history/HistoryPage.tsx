import { useEffect, useMemo, useState } from "react";
import { FileAudio, LoaderCircle, Search, SearchX, Upload, X, CircleCheck, CircleAlert, Ban, Clock } from "lucide-react";
import { api } from "../../ipc/commands";
import type { ImportJob } from "../../ipc/types";
import { attempt, loadHistory, store, toast, useStore } from "../../state/store";
import { Button, Keycaps } from "../../components/ui";
import { groupByDay } from "../../lib/format";
import { IMPORT_STATE_LABEL } from "../../lib/status";
import { EntryCard } from "./EntryCard";
import { StorageBanner } from "../home/HomePage";
import { useDictationShortcut } from "../../app/Sidebar";
import { cx } from "../../lib/cx";

export function HistoryPage() {
  const history = useStore((s) => s.history);
  const loaded = useStore((s) => s.historyLoaded);
  const storedQuery = useStore((s) => s.historyQuery);
  const [query, setQuery] = useState(storedQuery);
  // Debounced search: 150 ms after the last keystroke.
  useEffect(() => {
    if (query === store.get().historyQuery) return;
    const t = window.setTimeout(() => {
      store.set({ historyQuery: query });
      void loadHistory(query);
    }, 150);
    return () => clearTimeout(t);
  }, [query]);

  // Leaving History resets the search so Home shows the latest entries.
  useEffect(
    () => () => {
      if (store.get().historyQuery) {
        store.set({ historyQuery: "" });
        void loadHistory("");
      }
    },
    [],
  );

  const groups = useMemo(() => groupByDay(history, (e) => e.createdAtUtc), [history]);
  const searching = storedQuery.trim().length > 0;

  return (
    <div className="page history">
      <div className="history-toolbar" role="search">
        <label className="search">
          <Search size={15} aria-hidden className="search-icon" />
          <span className="sr-only">Search transcripts and apps</span>
          <input
            className="search-input"
            type="search"
            value={query}
            placeholder="Search transcripts and apps"
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape" && query) {
                e.preventDefault();
                setQuery("");
              }
            }}
            spellCheck={false}
          />
          {query && (
            <button type="button" className="search-clear" aria-label="Clear search" onClick={() => setQuery("")}>
              <X size={13} aria-hidden />
            </button>
          )}
        </label>
        <Button
          variant="secondary"
          icon={<Upload size={14} aria-hidden />}
          onClick={async () => {
            const n = await attempt("Couldn't import audio", api.importAudio);
            if (n) toast(n === 1 ? "1 file queued" : `${n} files queued`, "info", "Imports are transcribed on this computer, one at a time.");
          }}
        >
          Import audio
        </Button>
      </div>

      <StorageBanner />
      <ImportStrip />

      <div className="sr-only" aria-live="polite">
        {loaded && searching ? `${history.length} ${history.length === 1 ? "result" : "results"}` : ""}
      </div>

      {!loaded ? (
        <div className="history-loading" role="status">
          <LoaderCircle size={16} className="spin" aria-hidden /> Loading History…
        </div>
      ) : history.length === 0 ? (
        searching ? (
          <NoMatches query={storedQuery} onClear={() => setQuery("")} />
        ) : (
          <NoRecordings />
        )
      ) : (
        groups.map((g) => (
          <section key={g.key} className="day-group" aria-label={g.label}>
            <h2 className="day-label">
              <span>{g.label}</span>
              <span className="day-count">{g.items.length}</span>
            </h2>
            <ul className="entry-list">
              {g.items.map((e) => (
                <li key={e.id}>
                  <EntryCard entry={e} />
                </li>
              ))}
            </ul>
          </section>
        ))
      )}
    </div>
  );
}

function ImportStrip() {
  const imports = useStore((s) => s.snapshot!.imports);
  const visible = imports.filter((j) => j.state !== "done" && j.state !== "canceled").length > 0 ? imports : [];
  if (visible.length === 0) return null;
  const active = imports.some((j) => j.state === "copying" || j.state === "transcribing");
  return (
    <section className="import-strip" aria-label="Import queue">
      <div className="import-strip-head">
        <span className="import-strip-title">
          <FileAudio size={14} aria-hidden /> Importing {visible.filter((j) => j.state !== "done").length} of {visible.length}
        </span>
        {active && (
          <Button size="sm" variant="ghost" icon={<X size={12} aria-hidden />} onClick={() => void attempt("Couldn't cancel import", api.cancelImport)}>
            Cancel current
          </Button>
        )}
      </div>
      <ul className="import-list">
        {visible.map((j) => (
          <ImportRow key={j.jobId} job={j} />
        ))}
      </ul>
    </section>
  );
}

function ImportRow({ job }: { job: ImportJob }) {
  const icon =
    job.state === "done" ? (
      <CircleCheck size={13} aria-hidden />
    ) : job.state === "failed" ? (
      <CircleAlert size={13} aria-hidden />
    ) : job.state === "canceled" ? (
      <Ban size={13} aria-hidden />
    ) : job.state === "queued" ? (
      <Clock size={13} aria-hidden />
    ) : (
      <LoaderCircle size={13} className="spin" aria-hidden />
    );
  return (
    <li className={cx("import-row", `is-${job.state}`)}>
      <span className="import-name" title={job.fileName}>
        {job.fileName}
      </span>
      <span className="import-state" role="status">
        {icon}
        {IMPORT_STATE_LABEL[job.state]}
        {job.error && <span className="import-error"> — {job.error}</span>}
      </span>
    </li>
  );
}

function NoRecordings() {
  const os = useStore((s) => s.snapshot!.platform.os);
  const { verb, accelerator } = useDictationShortcut();
  return (
    <div className="empty">
      <div className="empty-illo" aria-hidden>
        <span className="empty-card c1" />
        <span className="empty-card c2" />
        <span className="empty-card c3">
          <span className="empty-wave">
            {[0.4, 0.7, 1, 0.6, 0.85, 0.5, 0.3].map((h, i) => (
              <span key={i} style={{ height: `${h * 100}%` }} />
            ))}
          </span>
        </span>
      </div>
      <h2 className="empty-title">No recordings yet</h2>
      <p className="empty-text">
        Everything you dictate is kept here, on this computer. {verb} <Keycaps accelerator={accelerator} os={os} size="sm" /> in any app to start, or import an audio file.
      </p>
    </div>
  );
}

function NoMatches({ query, onClear }: { query: string; onClear: () => void }) {
  return (
    <div className="empty is-search">
      <div className="empty-icon" aria-hidden>
        <SearchX size={22} />
      </div>
      <h2 className="empty-title">No matches for “{query}”</h2>
      <p className="empty-text">Search looks through transcript text and app names. Try a different word.</p>
      <Button variant="secondary" size="sm" onClick={onClear}>
        Clear search
      </Button>
    </div>
  );
}
