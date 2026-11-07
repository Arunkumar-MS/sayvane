import { useEffect, useRef, useState } from "react";
import { CircleAlert, CircleCheck, Globe, LoaderCircle } from "lucide-react";
import { api } from "../../ipc/commands";
import type { Os, ShortcutAction, ShortcutStatus } from "../../ipc/types";
import { errorText, saveSettings, store, toast, useStore } from "../../state/store";
import { Callout, Keycaps, Row, Section, Switch } from "../../components/ui";
import { captureAccelerator, acceleratorKeycaps } from "../../lib/accelerator";
import { cx } from "../../lib/cx";

const ACTIONS: Array<{ action: ShortcutAction; label: string; description: string }> = [
  { action: "pushToTalk", label: "Hold to talk", description: "Hold to record, release to insert." },
  { action: "toggleHandsFree", label: "Hands-free recording", description: "Press once to start, again to finish." },
  { action: "pasteLast", label: "Paste last transcript", description: "Inserts your most recent transcript into the focused field." },
  { action: "copyLast", label: "Copy last transcript", description: "Copies your most recent transcript to the clipboard." },
  { action: "cancel", label: "Cancel", description: "Stops the current recording or processing. Only active while dictating." },
];

export function ShortcutsTab() {
  const os = useStore((s) => s.snapshot!.platform.os);
  const bindings = useStore((s) => s.snapshot!.settings.shortcuts);
  const statuses = useStore((s) => s.snapshot!.shortcuts);

  return (
    <>
      {os === "macos" && <FnCard />}
      <Section
        title="Keyboard shortcuts"
        description={
          os === "macos"
            ? "These work in any app, alongside the fn gesture. Click a shortcut, then press the new keys."
            : "These work in any app. Click a shortcut, then press the new keys. If another app already uses a combination, you'll see it here."
        }
      >
        <div className="group">
          {ACTIONS.map((a) => {
            const binding = bindings.find((b) => b.action === a.action);
            const status = statuses.find((s) => s.action === a.action);
            return <ShortcutRow key={a.action} os={os} {...a} accelerator={binding?.accelerator ?? ""} status={status} />;
          })}
        </div>
      </Section>
    </>
  );
}

function ShortcutRow({ os, action, label, description, accelerator, status }: { os: Os; action: ShortcutAction; label: string; description: string; accelerator: string; status?: ShortcutStatus }) {
  const [capturing, setCapturing] = useState(false);
  const [preview, setPreview] = useState<string[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);

  const stop = () => {
    setCapturing(false);
    setPreview([]);
    void api.suspendShortcuts(false).catch(() => undefined);
  };

  useEffect(() => {
    if (!capturing) return;
    const onKey = async (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.repeat) return;
      const r = captureAccelerator(e, os);
      if (r.kind === "pending") {
        setPreview(r.preview.length ? acceleratorKeycaps(r.preview.join("+"), os) : []);
        return;
      }
      if (r.kind === "cancel") {
        setProblem(null);
        stop();
        return;
      }
      if (r.kind === "invalid") {
        setProblem(r.reason);
        return;
      }
      setCapturing(false);
      setPreview([]);
      setSaving(true);
      setProblem(null);
      try {
        const shortcuts = await api.setShortcut(action, r.accelerator);
        const snap = store.get().snapshot;
        if (snap) store.set({ snapshot: { ...snap, shortcuts, settings: { ...snap.settings, shortcuts: snap.settings.shortcuts.map((b) => (b.action === action ? { action, accelerator: r.accelerator } : b)) } } });
        const st = shortcuts.find((s) => s.action === action);
        if (st && !st.registered) toast("Shortcut unavailable", "error", st.error ?? "Another app may be using it.");
        else toast("Shortcut saved");
      } catch (err) {
        setProblem(errorText(err));
      } finally {
        setSaving(false);
        void api.suspendShortcuts(false).catch(() => undefined);
        btn.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing, os, action]);

  const start = () => {
    setProblem(null);
    setCapturing(true);
    void api.suspendShortcuts(true).catch(() => undefined);
  };

  const registered = status ? status.registered : true;
  const statusEl = saving ? (
    <span className="sc-status is-pending">
      <LoaderCircle size={12} className="spin" aria-hidden /> Checking…
    </span>
  ) : registered ? (
    <span className="sc-status is-ok">
      <CircleCheck size={12} aria-hidden /> Active
    </span>
  ) : (
    <span className="sc-status is-error">
      <CircleAlert size={12} aria-hidden /> Not registered
    </span>
  );

  return (
    <Row
      label={label}
      description={
        <>
          {description}
          {!registered && status?.error && <span className="sc-error"> {status.error} Choose another combination.</span>}
          {problem && (
            <span className="sc-error" role="alert">
              {" "}
              {problem}
            </span>
          )}
        </>
      }
      control={
        <>
          {statusEl}
          <button
            ref={btn}
            type="button"
            className={cx("sc-capture", capturing && "is-capturing", !registered && "is-error")}
            onClick={() => (capturing ? stop() : start())}
            onBlur={() => capturing && stop()}
            aria-label={capturing ? `Recording new shortcut for ${label}. Press keys, or Escape to cancel.` : `${label} shortcut. Click to change.`}
            aria-live="polite"
          >
            {capturing ? (
              preview.length ? (
                <span className="keycaps keycaps-sm">
                  {preview.map((k, i) => (
                    <kbd key={i} className="keycap">
                      {k}
                    </kbd>
                  ))}
                  <span className="sc-plus">+ …</span>
                </span>
              ) : (
                <span className="sc-prompt">Press keys · Esc to cancel</span>
              )
            ) : accelerator ? (
              <Keycaps accelerator={accelerator} os={os} size="sm" />
            ) : (
              <span className="sc-prompt">Record shortcut</span>
            )}
          </button>
        </>
      }
    />
  );
}

function FnCard() {
  const enabled = useStore((s) => s.snapshot!.settings.fnGestureEnabled);
  return (
    <Section title="The fn key" description="The quickest way to dictate on a Mac.">
      <div className="group">
        <Row
          label="Use the fn key to dictate"
          description="Works in any app with a text field."
          control={<Switch hideLabel label="Use the fn key to dictate" checked={enabled} onChange={(v) => void saveSettings({ fnGestureEnabled: v })} />}
        />
        <div className={cx("fn-gestures", !enabled && "is-off")}>
          <Gesture keys={["fn"]} title="Hold" text="Talk while holding, release to insert." />
          <Gesture keys={["fn", "fn"]} title="Double-tap" text="Hands-free. Tap fn again to finish." />
          <Gesture keys={["fn", "Space"]} plus title="fn + Space" text="Start or stop hands-free." />
        </div>
        <div className="row-callout">
          <Callout tone="info" icon={<Globe size={16} aria-hidden />} title="Stop fn from opening emoji or dictation">
            In System Settings › Keyboard, set <strong>“Press 🌐 key to”</strong> to <strong>Do Nothing</strong>. Otherwise macOS may react to fn at the same time.
          </Callout>
        </div>
      </div>
    </Section>
  );
}

function Gesture({ keys, title, text, plus }: { keys: string[]; title: string; text: string; plus?: boolean }) {
  return (
    <div className="gesture">
      <div className="gesture-keys" aria-hidden>
        {keys.map((k, i) => (
          <span key={i} className="gesture-key-wrap">
            {i > 0 && <span className="gesture-sep">{plus ? "+" : ""}</span>}
            <kbd className={cx("keycap gesture-key", k.length > 2 && "is-wide")}>{k === "fn" ? <><Globe size={10} aria-hidden /> fn</> : k}</kbd>
          </span>
        ))}
      </div>
      <div className="gesture-title">{title}</div>
      <div className="gesture-text">{text}</div>
    </div>
  );
}
