import { FileAudio, Mic, NotebookPen } from "lucide-react";
import type { DictationSource } from "../../ipc/types";

export const SOURCE_LABEL: Record<DictationSource, string> = {
  dictation: "Dictation",
  note: "Note",
  import: "Imported file",
};

export function SourceIcon({ source, size = 14 }: { source: DictationSource; size?: number }) {
  if (source === "note") return <NotebookPen size={size} aria-hidden />;
  if (source === "import") return <FileAudio size={size} aria-hidden />;
  return <Mic size={size} aria-hidden />;
}
