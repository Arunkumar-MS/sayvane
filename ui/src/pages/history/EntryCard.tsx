import { memo, useState } from "react";
import { Copy, Download, RotateCcw, Trash2, Undo2, ChevronDown, Check } from "lucide-react";
import { api } from "../../ipc/commands";
import type { ExportFormat, HistoryEntry, TextVariant } from "../../ipc/types";
import { attempt, toast } from "../../state/store";
import { Button, Callout, Menu, Pill, Segmented, Tooltip, type MenuItem } from "../../components/ui";
import { entryStatus } from "../../lib/status";
import { formatDuration, formatTime } from "../../lib/format";
import { SOURCE_LABEL, SourceIcon } from "./SourceIcon";
import { cx } from "../../lib/cx";

const BUSY = new Set(["recording", "queued", "transcribing", "polishing"]);

export const EntryCard = memo(function EntryCard({ entry: e }: { entry: HistoryEntry }) {
  const [variant, setVariant] = useState<TextVariant>("final");
  const [confirming, setConfirming] = useState(false);
  const [copied, setCopied] = useState(false);
  const [working, setWorking] = useState<null | "retry" | "delete">(null);

  const hasPolish = e.polishedText !== null && e.polishedText !== e.originalText;
  const v: TextVariant = hasPolish ? variant : "final";
  const text = v === "original" ? e.originalText : v === "polished" ? (e.polishedText ?? "") : e.finalText;
  const busy = BUSY.has(e.processingState);
  const pills = entryStatus(e);
  const time = formatTime(e.createdAtUtc);
  const where = e.source === "import" ? e.importName : e.destinationAppName;
  const showError = e.processingState === "failed" || e.processingState === "interrupted" || e.processingState === "noSpeech" || !!e.errorMessage;

  const copy = async () => {
    const ok = await attempt("Couldn't copy", () => api.copyEntry(e.id, v));
    if (ok !== undefined) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    }
  };

  const retry = async () => {
    setWorking("retry");
    await attempt(e.retryKind === "recover" ? "Couldn't recover" : "Couldn't retry", () => api.recoverEntry(e.id));
    setWorking(null);
  };

  const exportAs = async (format: ExportFormat) => {
    const saved = await attempt("Export failed", () => api.exportEntry(e.id, format, format === "txt" ? v : "final"));
    if (saved) toast(`Exported ${format.toUpperCase()}`);
  };

  const del = async () => {
    setWorking("delete");
    await attempt("Couldn't delete", () => api.deleteEntry(e.id));
    setWorking(null);
    setConfirming(false);
  };

  const hasCallout = showError || (e.canRetry && e.retryKind === "recover");
  const timingReason = "Needs time-aligned segments, which this entry doesn't have";
  const exportItems: MenuItem[] = [
    { key: "txt", label: "Plain text (.txt)", hint: hasPolish ? `${v === "final" ? "Final" : v === "original" ? "Original" : "Polished"} text` : undefined, onSelect: () => void exportAs("txt") },
    { key: "json", label: "JSON (.json)", hint: "All text variants, language and timings", onSelect: () => void exportAs("json") },
    { key: "srt", label: "Subtitles (.srt)", disabled: !e.hasSegments, disabledReason: timingReason, hint: "From the original timed transcript", onSelect: () => void exportAs("srt") },
    { key: "vtt", label: "WebVTT (.vtt)", disabled: !e.hasSegments, disabledReason: timingReason, hint: "From the original timed transcript", onSelect: () => void exportAs("vtt") },
    { key: "tsv", label: "Table (.tsv)", disabled: !e.hasSegments, disabledReason: timingReason, hint: "start, end, text per segment", onSelect: () => void exportAs("tsv") },
  ];

  const emptyText =
    e.processingState === "failed"
      ? "No transcript yet — the audio is saved."
      : e.processingState === "canceled"
        ? "Recording was canceled before transcription."
        : e.processingState === "noSpeech"
          ? "No speech detected in this recording."
          : busy
            ? "Working on it…"
            : "No transcript.";

  return (
    <article className={cx("entry", busy && "is-busy")} aria-label={`${SOURCE_LABEL[e.source]} at ${time}${where ? `, ${where}` : ""}`}>
      <header className="entry-head">
        <div className="entry-meta">
          <time className="entry-time" dateTime={e.createdAtUtc}>
            {time}
          </time>
          <span className="entry-dot" aria-hidden>
            ·
          </span>
          <span className="entry-source">
            <SourceIcon source={e.source} size={13} />
            {where ?? SOURCE_LABEL[e.source]}
          </span>
          {e.durationMs > 0 && (
            <>
              <span className="entry-dot" aria-hidden>
                ·
              </span>
              <span className="entry-duration" aria-label={`Duration ${formatDuration(e.durationMs)}`}>
                {formatDuration(e.durationMs)}
              </span>
            </>
          )}
        </div>
        <div className="entry-pills">
          {pills.map((p, i) => (
            <Pill key={i} pill={p} />
          ))}
        </div>
      </header>

      {hasPolish && (
        <div className="entry-variants">
          <Segmented<TextVariant>
            size="sm"
            label="Text version"
            value={v}
            onChange={setVariant}
            options={[
              { value: "final", label: "Final" },
              { value: "original", label: "Original" },
              { value: "polished", label: "Polished" },
            ]}
          />
          <span className="entry-variant-note">
            {v === "final" ? "What was inserted" : v === "original" ? "Local transcript, before AI polish" : "Returned by your AI provider"}
          </span>
        </div>
      )}

      {text.trim() ? <p className="entry-text selectable">{text}</p> : !hasCallout && <p className="entry-text is-empty">{emptyText}</p>}

      {hasCallout && (
        <Callout
          tone={e.processingState === "failed" ? "error" : e.processingState === "canceled" ? "neutral" : "warning"}
          className="entry-error"
          action={
            e.canRetry && e.retryKind === "recover" ? (
              <Button size="sm" variant="secondary" icon={<Undo2 size={13} aria-hidden />} onClick={() => void retry()} loading={working === "retry"} disabled={busy}>
                Recover
              </Button>
            ) : undefined
          }
        >
          {e.errorMessage ??
            (e.processingState === "interrupted"
              ? "Recording stopped unexpectedly. The audio up to that point was kept."
              : e.processingState === "canceled"
                ? "You canceled this recording, but the audio was kept. Recover transcribes it now."
                : "The audio is saved. Recover transcribes it again.")}
        </Callout>
      )}

      <footer className={cx("entry-actions", confirming && "is-confirming")} aria-label="Entry actions">
        {confirming ? (
          <div className="confirm" role="group" aria-label="Confirm delete">
            <span className="confirm-text">
              Delete this entry? This removes the transcript and its recorded audio.
              {e.source === "import" && " Your original imported file is not touched."}
            </span>
            <Button size="sm" variant="ghost" onClick={() => setConfirming(false)} autoFocus>
              Keep
            </Button>
            <Button size="sm" variant="danger" loading={working === "delete"} icon={<Trash2 size={13} aria-hidden />} onClick={() => void del()}>
              Delete
            </Button>
          </div>
        ) : (
          <>
            <Button size="sm" variant="ghost" icon={copied ? <Check size={13} aria-hidden /> : <Copy size={13} aria-hidden />} onClick={() => void copy()} disabled={!text.trim()}>
              {copied ? "Copied" : "Copy"}
            </Button>
            {e.canRetry && e.retryKind !== "recover" ? (
              <Button size="sm" variant="ghost" icon={<RotateCcw size={13} aria-hidden />} onClick={() => void retry()} loading={working === "retry"} disabled={busy}>
                Retry
              </Button>
            ) : (
              !e.canRetry &&
              e.retryUnavailableReason && (
                <Tooltip text={e.retryUnavailableReason}>
                  <Button size="sm" variant="ghost" icon={<RotateCcw size={13} aria-hidden />} aria-disabled="true" className="is-unavailable">
                    Retry
                    <span className="sr-only"> unavailable: {e.retryUnavailableReason}</span>
                  </Button>
                </Tooltip>
              )
            )}
            <Menu
              label="Export format"
              items={exportItems}
              align="end"
              trigger={({ open, toggle, id }) => (
                <Button size="sm" variant="ghost" icon={<Download size={13} aria-hidden />} aria-haspopup="menu" aria-expanded={open} aria-controls={open ? id : undefined} onClick={toggle}>
                  Export
                  <ChevronDown size={12} aria-hidden className="chev" />
                </Button>
              )}
            />
            <span className="entry-actions-sep" aria-hidden />
            <Button size="sm" variant="ghost" className="entry-delete" icon={<Trash2 size={13} aria-hidden />} onClick={() => setConfirming(true)} disabled={busy}>
              Delete
            </Button>
          </>
        )}
      </footer>
    </article>
  );
});
