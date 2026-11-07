import { useId, useState } from "react";
import { CircleCheck, CircleAlert, CircleHelp, FolderOpen, HardDrive, Keyboard, Mic, ShieldCheck, Trash2, MinusCircle } from "lucide-react";
import { api } from "../../ipc/commands";
import type { PermissionKind, PermissionState, RetentionPolicy } from "../../ipc/types";
import { attempt, refreshSnapshot, saveSettings, toast, useStore } from "../../state/store";
import { Button, Row, Section, Segmented, Switch } from "../../components/ui";
import { cx } from "../../lib/cx";

const RETENTION: Array<{ value: RetentionPolicy; label: string; description: string }> = [
  { value: "keep", label: "Keep until I delete it", description: "Recommended. Lets you Recover or Retry any recording later." },
  { value: "afterTranscription", label: "Delete audio after transcription", description: "Transcripts stay. Retry won't be possible once audio is gone." },
  { value: "days7", label: "Keep audio for 7 days", description: "Older audio is removed automatically; transcripts stay." },
  { value: "days30", label: "Keep audio for 30 days", description: "Older audio is removed automatically; transcripts stay." },
];

function PermissionBadge({ state }: { state: PermissionState }) {
  if (state === "granted")
    return (
      <span className="perm is-ok">
        <CircleCheck size={13} aria-hidden /> Allowed
      </span>
    );
  if (state === "denied")
    return (
      <span className="perm is-bad">
        <CircleAlert size={13} aria-hidden /> Not allowed
      </span>
    );
  if (state === "notApplicable")
    return (
      <span className="perm">
        <MinusCircle size={13} aria-hidden /> Not needed
      </span>
    );
  return (
    <span className="perm is-warn">
      <CircleHelp size={13} aria-hidden /> {state === "notDetermined" ? "Not asked yet" : "Unknown"}
    </span>
  );
}

export function PrivacyTab() {
  const os = useStore((s) => s.snapshot!.platform.os);
  const perms = useStore((s) => s.snapshot!.permissions);
  const settings = useStore((s) => s.snapshot!.settings);
  const storage = useStore((s) => s.snapshot!.storage);
  const brand = useStore((s) => s.snapshot!.brand.displayName);

  const permRows: Array<{ kind: PermissionKind; label: string; description: string; icon: typeof Mic }> = [
    { kind: "microphone", label: "Microphone", description: "To hear you. Audio is processed on this computer.", icon: Mic },
  ];
  if (os === "macos") {
    permRows.push({ kind: "accessibility", label: "Accessibility", description: `Lets ${brand} insert text into the app you're using.`, icon: Keyboard });
    if (perms.inputMonitoring !== "notApplicable")
      permRows.push({ kind: "inputMonitoring", label: "Input Monitoring", description: "Lets the fn key and shortcuts work in every app. Typed text is never recorded.", icon: Keyboard });
  }

  return (
    <>
      <div className="privacy-hero">
        <span className="privacy-hero-icon">
          <ShieldCheck size={20} aria-hidden />
        </span>
        <div>
          <h2 className="privacy-hero-title">Your voice stays on this computer</h2>
          <ul className="privacy-points">
            <li>Audio is recorded and transcribed locally. It never leaves this computer.</li>
            <li>With AI polish off, nothing is sent anywhere. With it on, only transcript text goes to the provider you choose.</li>
            <li>History is stored in your user folder, protected by your account. Nothing is shared unless you export it.</li>
          </ul>
        </div>
      </div>

      <Section
        title="System access"
        description={os === "macos" ? "Managed in System Settings › Privacy & Security." : "Managed in Windows Settings › Privacy & security."}
        aside={
          <Button size="sm" variant="ghost" onClick={() => void attempt("Couldn't refresh", async () => { await api.refreshPermissions(); await refreshSnapshot(); })}>
            Check again
          </Button>
        }
      >
        <div className="group">
          {permRows.map((r) => {
            const Icon = r.icon;
            const st = perms[r.kind];
            return (
              <Row
                key={r.kind}
                label={
                  <span className="perm-label">
                    <Icon size={14} aria-hidden /> {r.label}
                  </span>
                }
                description={r.description}
                control={
                  <>
                    <PermissionBadge state={st} />
                    {st !== "granted" && st !== "notApplicable" && (
                      <Button
                        size="sm"
                        variant={st === "notDetermined" ? "primary" : "secondary"}
                        onClick={() => void attempt("Couldn't open settings", () => (st === "notDetermined" ? api.requestPermission(r.kind) : api.openPermissionSettings(r.kind)))}
                      >
                        {st === "notDetermined" ? "Allow" : "Open settings"}
                      </Button>
                    )}
                  </>
                }
              />
            );
          })}
        </div>
        {os === "macos" && (perms.microphone !== "granted" || perms.accessibility !== "granted") && (
          <div className="perm-reset">
            <p>
              Switched on in System Settings but still shows Not allowed? After installing a new version, macOS can keep
              the old app's access. Reset it and allow access again.
            </p>
            <Button
              size="sm"
              variant="secondary"
              onClick={() =>
                void attempt("Couldn't reset access", async () => {
                  const msg = await api.resetPermissions();
                  toast(msg);
                  await refreshSnapshot();
                })
              }
            >
              Reset access
            </Button>
          </div>
        )}
      </Section>

      <Section title="Recordings" description="Transcripts are always kept until you delete them. Choose how long the audio is kept.">
        <div className="group radio-group" role="radiogroup" aria-label="Audio retention">
          {RETENTION.map((r) => (
            <label key={r.value} className={cx("radio-row", settings.retention === r.value && "is-checked")}>
              <input type="radio" name="retention" value={r.value} checked={settings.retention === r.value} onChange={() => void saveSettings({ retention: r.value })} />
              <span className="radio-dot" aria-hidden />
              <span className="row-text">
                <span className="row-label">{r.label}</span>
                <span className="row-desc">{r.description}</span>
              </span>
            </label>
          ))}
        </div>
      </Section>

      <Section title="Storage">
        <div className="group">
          <Row
            label={
              <span className="perm-label">
                <HardDrive size={14} aria-hidden /> Data folder
              </span>
            }
            description={<span className="mono selectable path">{storage.dataDir}</span>}
            control={
              <Button size="sm" variant="secondary" icon={<FolderOpen size={13} aria-hidden />} onClick={() => void attempt("Couldn't open the folder", api.revealDataDir)}>
                {os === "macos" ? "Show in Finder" : "Show in Explorer"}
              </Button>
            }
          />
        </div>
      </Section>

      <Section title="General">
        <div className="group">
          <Row label="Open at login" description={`Start ${brand} quietly in the ${os === "macos" ? "menu bar" : "system tray"} when you sign in.`} control={<Switch hideLabel label="Open at login" checked={settings.launchAtLogin} onChange={(v) => void saveSettings({ launchAtLogin: v })} />} />
          <Row label="Sounds" description="A soft tone when recording starts and stops." control={<Switch hideLabel label="Sounds" checked={settings.soundsEnabled} onChange={(v) => void saveSettings({ soundsEnabled: v })} />} />
          <Row
            label="Motion"
            description="Reduce animation in the app and the recording bar."
            control={
              <Segmented
                size="sm"
                label="Motion"
                value={settings.reducedMotion}
                onChange={(v) => void saveSettings({ reducedMotion: v })}
                options={[
                  { value: "system", label: "System" },
                  { value: "reduce", label: "Reduced" },
                  { value: "full", label: "Full" },
                ]}
              />
            }
          />
        </div>
      </Section>

      <DangerZone />
    </>
  );
}

function DangerZone() {
  const [open, setOpen] = useState(false);
  const [phrase, setPhrase] = useState("");
  const [busy, setBusy] = useState(false);
  const id = useId();
  const ok = phrase === "DELETE";
  return (
    <Section title="Delete data">
      <div className="danger">
        <div className="danger-head">
          <div className="row-text">
            <div className="row-label">Delete all recordings and transcripts</div>
            <div className="row-desc">Permanently removes everything in History from this computer. Imported original files are never touched.</div>
          </div>
          {!open && (
            <Button variant="secondary" className="danger-outline" icon={<Trash2 size={13} aria-hidden />} onClick={() => setOpen(true)}>
              Delete all data…
            </Button>
          )}
        </div>
        {open && (
          <div className="danger-body">
            <div className="danger-lists">
              <div>
                <div className="danger-list-title">Will be deleted</div>
                <ul>
                  <li>All recorded audio</li>
                  <li>All transcripts, original and polished</li>
                  <li>Statistics and History search</li>
                </ul>
              </div>
              <div>
                <div className="danger-list-title">Will be kept</div>
                <ul>
                  <li>Your settings and shortcuts</li>
                  <li>Installed speech models</li>
                  <li>Saved provider keys (remove them in AI Polish)</li>
                </ul>
              </div>
            </div>
            <label className="field-label" htmlFor={id}>
              Type <strong className="mono">DELETE</strong> to confirm. This can't be undone.
            </label>
            <div className="danger-confirm">
              <input id={id} className="input mono" value={phrase} onChange={(e) => setPhrase(e.target.value)} autoComplete="off" spellCheck={false} placeholder="DELETE" />
              <Button
                variant="danger"
                disabled={!ok}
                loading={busy}
                onClick={async () => {
                  setBusy(true);
                  const r = await attempt("Couldn't delete data", () => api.deleteAllData(phrase));
                  setBusy(false);
                  if (r !== undefined) {
                    toast("All recordings and transcripts deleted");
                    setOpen(false);
                    setPhrase("");
                  }
                }}
              >
                Delete everything
              </Button>
              <Button
                variant="ghost"
                onClick={() => {
                  setOpen(false);
                  setPhrase("");
                }}
              >
                Cancel
              </Button>
            </div>
          </div>
        )}
      </div>
    </Section>
  );
}
