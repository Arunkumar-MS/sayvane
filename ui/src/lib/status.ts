// Maps core enums to friendly, non-color-only status labels (unit tested).
import type { HistoryEntry, ImportJob, SessionPhase, SessionView } from "../ipc/types";

export type Tone = "success" | "info" | "warning" | "error" | "neutral" | "progress";
export type StatusIcon = "check" | "send" | "save" | "alert" | "x" | "info" | "spinner" | "sparkles" | "ban" | "mic" | "clock" | "mute";

export interface StatusPill {
  label: string;
  tone: Tone;
  icon: StatusIcon;
  /** Longer explanation for tooltips / screen readers. */
  hint?: string;
}

function hasAudio(e: HistoryEntry): boolean {
  return e.audioState === "saved" || e.audioState === "interrupted";
}

/** Primary status pill for an entry, followed by secondary pills (e.g. polish outcome). */
export function entryStatus(e: HistoryEntry): StatusPill[] {
  switch (e.processingState) {
    case "recording":
      return [{ label: "Recording", tone: "progress", icon: "mic" }];
    case "queued":
      return [{ label: "Queued", tone: "progress", icon: "clock" }];
    case "transcribing":
      return [{ label: "Transcribing locally", tone: "progress", icon: "spinner" }];
    case "polishing":
      return [{ label: "Polishing", tone: "progress", icon: "spinner" }];
    case "failed":
      return [
        {
          label: hasAudio(e) ? "Failed — audio saved" : "Failed",
          tone: "error",
          icon: "alert",
          hint: e.errorMessage ?? undefined,
        },
      ];
    case "noSpeech":
      return [{ label: "No speech detected", tone: "warning", icon: "mute" }];
    case "canceled":
      return [hasAudio(e) ? { label: "Canceled recording", tone: "neutral", icon: "ban", hint: "Audio was kept, so you can still recover it." } : { label: "Canceled", tone: "neutral", icon: "ban" }];
    case "interrupted":
      return [
        {
          label: hasAudio(e) ? "Interrupted — audio saved" : "Interrupted",
          tone: "warning",
          icon: "alert",
          hint: "Recording stopped unexpectedly (sleep, lock, or device change).",
        },
      ];
    case "done":
      break;
  }

  const pills: StatusPill[] = [deliveryStatus(e)];
  if (e.polishState === "applied") pills.push({ label: "Polished", tone: "info", icon: "sparkles" });
  if (e.polishState === "fallback")
    pills.push({ label: "AI unavailable; original used", tone: "warning", icon: "alert", hint: "The AI provider did not respond in time, so the local transcript was used unchanged." });
  if (e.polishState === "pending") pills.push({ label: "Polishing", tone: "progress", icon: "spinner" });
  return pills;
}

function deliveryStatus(e: HistoryEntry): StatusPill {
  const app = e.destinationAppName;
  switch (e.insertionState) {
    case "confirmed":
      return { label: app ? `Inserted in ${app}` : "Inserted", tone: "success", icon: "check" };
    case "sentUnconfirmed":
      return { label: "Paste sent", tone: "info", icon: "send", hint: "The text was pasted but the app did not confirm it. Check before pasting again." };
    case "savedOnly":
      return { label: "Saved to History", tone: "neutral", icon: "save" };
    case "destinationChanged":
      return { label: "App changed — not inserted", tone: "warning", icon: "alert", hint: "You switched apps before the text was ready, so it was kept here instead." };
    case "ineligible":
      return { label: "Field not editable — saved", tone: "warning", icon: "alert", hint: "The focused field could not accept text. Copy it or use Paste last." };
    case "canceled":
      return { label: "Insertion canceled", tone: "neutral", icon: "ban" };
    case "notRequested":
      return e.source === "import"
        ? { label: "Imported", tone: "neutral", icon: "check" }
        : { label: "Saved to History", tone: "neutral", icon: "save" };
  }
}

export const PHASE_LABEL: Record<SessionPhase, string> = {
  idle: "Ready",
  starting: "Opening microphone",
  recording: "Recording",
  savingAudio: "Saving audio",
  transcribing: "Transcribing locally",
  polishing: "Polishing",
  savingTranscript: "Saving",
  delivering: "Inserting text",
  result: "Done",
};

export function sessionStatus(s: SessionView): StatusPill {
  switch (s.phase) {
    case "idle":
      return { label: "Ready", tone: "neutral", icon: "check" };
    case "starting":
      return { label: PHASE_LABEL.starting, tone: "progress", icon: "spinner" };
    case "recording":
      return { label: s.mode === "note" ? "Recording note" : "Recording", tone: "error", icon: "mic" };
    case "polishing":
      return { label: "Polishing", tone: "progress", icon: "sparkles" };
    case "result": {
      const r = s.result;
      if (!r) return { label: "Done", tone: "success", icon: "check" };
      const icon: StatusIcon = r.tone === "success" ? "check" : r.tone === "info" ? "info" : r.tone === "warning" ? "alert" : "x";
      return { label: r.message, tone: r.tone, icon };
    }
    default:
      return { label: PHASE_LABEL[s.phase], tone: "progress", icon: "spinner" };
  }
}

export function isSessionActive(s: SessionView): boolean {
  return s.phase !== "idle" && s.phase !== "result";
}

export const IMPORT_STATE_LABEL: Record<ImportJob["state"], string> = {
  queued: "Queued",
  copying: "Copying",
  transcribing: "Transcribing",
  done: "Done",
  failed: "Failed",
  canceled: "Canceled",
};
