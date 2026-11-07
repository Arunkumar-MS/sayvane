import { CircleCheck, Circle, Keyboard, Mic, Download, TriangleAlert } from "lucide-react";
import type { CSSProperties, ReactNode } from "react";
import { api } from "../../ipc/commands";
import type { PermissionKind, PermissionState } from "../../ipc/types";
import { attempt, useStore } from "../../state/store";
import { Button, Progress } from "../../components/ui";
import { formatBytes } from "../../lib/format";
import { cx } from "../../lib/cx";

interface Item {
  key: string;
  icon: ReactNode;
  title: string;
  description: ReactNode;
  done: boolean;
  problem?: boolean;
  action?: ReactNode;
  progress?: ReactNode;
}

export function SetupChecklist() {
  const os = useStore((s) => s.snapshot!.platform.os);
  const perms = useStore((s) => s.snapshot!.permissions);
  const brand = useStore((s) => s.snapshot!.brand.displayName);
  const models = useStore((s) => s.models);
  const engine = useStore((s) => s.snapshot!.engine);

  const items: Item[] = [];

  const permAction = (kind: PermissionKind, state: PermissionState) =>
    state === "notDetermined" ? (
      <Button size="sm" variant="primary" onClick={() => void attempt("Couldn't request access", () => api.requestPermission(kind))}>
        Grant access
      </Button>
    ) : (
      <Button size="sm" variant="secondary" onClick={() => void attempt("Couldn't open settings", () => api.openPermissionSettings(kind))}>
        Open settings
      </Button>
    );

  const micOk = perms.microphone === "granted" || perms.microphone === "notApplicable";
  items.push({
    key: "mic",
    icon: <Mic size={15} aria-hidden />,
    title: "Microphone access",
    description:
      os === "macos"
        ? perms.microphone === "denied"
          ? `Turn on ${brand} in System Settings › Privacy & Security › Microphone.`
          : "Needed to hear you. Audio is processed on this computer."
        : perms.microphone === "denied"
          ? "Turn on microphone access in Windows Settings › Privacy & security › Microphone."
          : "Needed to hear you. Audio is processed on this computer.",
    done: micOk,
    problem: perms.microphone === "denied",
    action: micOk ? undefined : permAction("microphone", perms.microphone),
  });

  if (os === "macos") {
    const axOk = perms.accessibility === "granted" || perms.accessibility === "notApplicable";
    items.push({
      key: "ax",
      icon: <Keyboard size={15} aria-hidden />,
      title: "Text insertion access",
      description: `Lets ${brand} type into the app you're using. System Settings › Privacy & Security › Accessibility.`,
      done: axOk,
      problem: perms.accessibility === "denied",
      action: axOk ? undefined : permAction("accessibility", perms.accessibility),
    });
    if (perms.inputMonitoring !== "notApplicable") {
      const imOk = perms.inputMonitoring === "granted";
      items.push({
        key: "im",
        icon: <Keyboard size={15} aria-hidden />,
        title: "Keyboard shortcut access",
        description: "Lets the fn key and shortcuts start dictation from any app. System Settings › Privacy & Security › Input Monitoring.",
        done: imOk,
        problem: perms.inputMonitoring === "denied",
        action: imOk ? undefined : permAction("inputMonitoring", perms.inputMonitoring),
      });
    }
  }

  const anyInstalled = models.some((m) => m.installed);
  const def = models.find((m) => m.isDefault) ?? models.find((m) => m.id === "base");
  const dl = def?.download;
  const modelOk = anyInstalled || (engine.state !== "noModel" && models.length === 0);
  items.push({
    key: "model",
    icon: <Download size={15} aria-hidden />,
    title: "Local speech model",
    description: def
      ? `${def.label} · ${formatBytes(def.sizeBytes)} — downloaded once, then everything runs offline.`
      : "Download a speech model to transcribe on this computer.",
    done: modelOk,
    action:
      modelOk || !def ? undefined : dl && dl.state !== "failed" ? (
        <Button size="sm" variant="ghost" onClick={() => void attempt("Couldn't cancel", () => api.cancelModelInstall(def.id))}>
          Cancel
        </Button>
      ) : (
        <Button size="sm" variant="primary" icon={<Download size={13} aria-hidden />} onClick={() => void attempt("Couldn't start download", () => api.installModel(def.id))}>
          {dl?.state === "failed" ? "Retry download" : `Install ${def.label} (${formatBytes(def.sizeBytes)})`}
        </Button>
      ),
    progress:
      def && dl && dl.state !== "failed" ? (
        <div className="setup-progress">
          <Progress value={dl.receivedBytes / Math.max(1, dl.totalBytes)} indeterminate={dl.state === "verifying"} label={`Downloading ${def.label} model`} />
          <span className="setup-progress-text">
            {dl.state === "verifying" ? "Verifying…" : `${formatBytes(dl.receivedBytes)} of ${formatBytes(dl.totalBytes)}`}
          </span>
        </div>
      ) : dl?.state === "failed" ? (
        <div className="setup-error">{dl.error ?? "Download failed."}</div>
      ) : undefined,
  });

  const remaining = items.filter((i) => !i.done).length;
  if (remaining === 0) return null;
  const done = items.length - remaining;

  return (
    <section className="card setup" aria-labelledby="setup-title">
      <div className="setup-head">
        <div>
          <h2 id="setup-title" className="setup-title">
            Finish setting up
          </h2>
          <p className="setup-sub">A couple of steps before your first dictation.</p>
        </div>
        <div className="setup-count" aria-label={`${done} of ${items.length} steps done`}>
          <span className="setup-count-ring" style={{ "--p": done / items.length } as CSSProperties} aria-hidden />
          {done} of {items.length}
        </div>
      </div>
      <ol className="setup-list">
        {items.map((item) => (
          <li key={item.key} className={cx("setup-item", item.done && "is-done")}>
            <span className={cx("setup-status", item.done ? "is-done" : item.problem ? "is-problem" : "is-todo")}>
              {item.done ? <CircleCheck size={18} aria-hidden /> : item.problem ? <TriangleAlert size={16} aria-hidden /> : <Circle size={18} aria-hidden />}
              <span className="sr-only">{item.done ? "Done:" : item.problem ? "Needs attention:" : "To do:"}</span>
            </span>
            <div className="setup-text">
              <div className="setup-item-title">
                <span className="setup-item-icon">{item.icon}</span>
                {item.title}
              </div>
              <div className="setup-item-desc">{item.description}</div>
              {item.progress}
            </div>
            {item.action && <div className="setup-action">{item.action}</div>}
          </li>
        ))}
      </ol>
    </section>
  );
}
