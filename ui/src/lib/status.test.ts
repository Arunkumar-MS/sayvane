import { describe, expect, it } from "vitest";
import type { HistoryEntry, InsertionState, PolishState, ProcessingState } from "../ipc/types";
import { entryStatus, sessionStatus } from "./status";

const base: HistoryEntry = {
  id: "x",
  createdAtUtc: "2026-11-07T10:00:00Z",
  source: "dictation",
  audioState: "saved",
  durationMs: 1000,
  originalText: "hi",
  polishedText: null,
  finalText: "hi",
  detectedLanguage: "en",
  modelId: "base",
  destinationAppName: "Notes",
  processingState: "done",
  polishState: "off",
  insertionState: "confirmed",
  errorCode: null,
  errorMessage: null,
  hasSegments: false,
  canRetry: true,
  retryUnavailableReason: null,
  retryKind: "retry",
  importName: null,
};
const e = (p: Partial<HistoryEntry>) => ({ ...base, ...p });

describe("entryStatus", () => {
  it("maps every enum value to non-empty friendly text", () => {
    const proc: ProcessingState[] = ["recording", "queued", "transcribing", "polishing", "done", "noSpeech", "failed", "canceled", "interrupted"];
    const ins: InsertionState[] = ["notRequested", "confirmed", "sentUnconfirmed", "savedOnly", "destinationChanged", "ineligible", "canceled"];
    const pol: PolishState[] = ["off", "skipped", "applied", "fallback", "pending"];
    for (const p of proc)
      for (const i of ins)
        for (const s of pol) {
          const pills = entryStatus(e({ processingState: p, insertionState: i, polishState: s }));
          expect(pills.length).toBeGreaterThan(0);
          for (const pill of pills) {
            expect(pill.label.length).toBeGreaterThan(0);
            expect(pill.icon).toBeTruthy();
          }
        }
  });
  it("uses specific wording", () => {
    expect(entryStatus(e({ insertionState: "sentUnconfirmed" }))[0].label).toBe("Paste sent");
    expect(entryStatus(e({ insertionState: "confirmed" }))[0].label).toBe("Inserted in Notes");
    expect(entryStatus(e({ polishState: "fallback" })).map((p) => p.label)).toContain("AI unavailable; original used");
    expect(entryStatus(e({ processingState: "canceled", audioState: "saved" }))[0].label).toBe("Canceled recording");
    expect(entryStatus(e({ processingState: "canceled", audioState: "none" }))[0].label).toBe("Canceled");
    expect(entryStatus(e({ source: "import", insertionState: "notRequested" }))[0].label).toBe("Imported");
    expect(entryStatus(e({ processingState: "failed" }))[0].tone).toBe("error");
  });
});

describe("sessionStatus", () => {
  const s = { phase: "idle" as const, sessionId: null, generation: 0, mode: null, elapsedMs: null, destinationLabel: null, cancelPending: false, result: null, busyReason: null };
  it("labels phases", () => {
    expect(sessionStatus(s).label).toBe("Ready");
    expect(sessionStatus({ ...s, phase: "transcribing" }).label).toBe("Transcribing locally");
    expect(sessionStatus({ ...s, phase: "polishing" }).label).toBe("Polishing");
    expect(sessionStatus({ ...s, phase: "result", result: { tone: "warning", message: "AI unavailable; original text used", dictationId: null, canCopy: true } })).toMatchObject({
      label: "AI unavailable; original text used",
      icon: "alert",
    });
  });
});
