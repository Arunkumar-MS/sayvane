import { useEffect, useId, useMemo, useState } from "react";
import { Building2, CircleCheck, CircleAlert, KeyRound, Plug, Plus, RefreshCw, Server, Sparkles, Trash2, X, Globe, ShieldCheck } from "lucide-react";
import { api } from "../../ipc/commands";
import type { AuthMode, ConnectionResult, CredentialStatus, ModelSummary, ProviderKind, ProviderProfile } from "../../ipc/types";
import { attempt, errorText, setPolishEnabled, store, toast, useStore } from "../../state/store";
import { Button, Callout, Menu, Row, Section, Segmented, Switch } from "../../components/ui";
import { cx } from "../../lib/cx";

const PRESETS: Record<ProviderKind, { label: string; baseUrl: string; authMode: AuthMode; hint: string }> = {
  openrouter: { label: "OpenRouter", baseUrl: "https://openrouter.ai/api/v1", authMode: "bearer", hint: "Many models with one API key" },
  compatible: { label: "Custom compatible API", baseUrl: "", authMode: "bearer", hint: "Any chat-completions compatible endpoint" },
  organization: { label: "Organization gateway", baseUrl: "", authMode: "bearer", hint: "Your company's internal gateway" },
  local: { label: "Local server", baseUrl: "http://127.0.0.1:11434/v1", authMode: "none", hint: "A model server running on this computer" },
};

function KindIcon({ kind, size = 15 }: { kind: ProviderKind; size?: number }) {
  if (kind === "openrouter") return <Globe size={size} aria-hidden />;
  if (kind === "organization") return <Building2 size={size} aria-hidden />;
  if (kind === "local") return <Server size={size} aria-hidden />;
  return <Plug size={size} aria-hidden />;
}

function newProfile(kind: ProviderKind): ProviderProfile {
  const p = PRESETS[kind];
  return {
    id: "",
    label: p.label,
    kind,
    baseUrl: p.baseUrl,
    chatPath: "chat/completions",
    modelsPath: "models",
    modelId: "",
    authMode: p.authMode,
    headers: {},
    secretHeaderNames: [],
    timeoutMs: 15000,
  };
}

export function validateProfile(p: ProviderProfile): string | null {
  if (!p.label.trim()) return "Give this provider a name.";
  let url: URL;
  try {
    url = new URL(p.baseUrl);
  } catch {
    return "Base URL must be a full address, like https://api.example.com/v1.";
  }
  const loopback = ["127.0.0.1", "localhost", "[::1]", "::1"].includes(url.hostname);
  if (url.protocol === "http:" && !loopback) return "Use https:// for remote servers. Plain http:// is only allowed for this computer (127.0.0.1).";
  if (url.protocol !== "https:" && url.protocol !== "http:") return "Base URL must start with https://.";
  if (!p.chatPath.trim()) return "Chat path is required (usually chat/completions).";
  if (!p.modelId.trim()) return "Choose or type a model ID.";
  if (!(p.timeoutMs >= 1000 && p.timeoutMs <= 120000)) return "Timeout must be between 1 and 120 seconds.";
  return null;
}

export function PolishTab() {
  const settings = useStore((s) => s.snapshot!.settings);
  const notice = useStore((s) => s.polishNotice);
  const [profiles, setProfiles] = useState<ProviderProfile[] | null>(null);
  const [editing, setEditing] = useState<ProviderProfile | null>(null);

  const reload = async () => {
    const list = await attempt("Couldn't load providers", api.listProfiles);
    if (list) setProfiles(list);
    return list;
  };
  useEffect(() => {
    void reload().then((list) => {
      if (list && list.length === 0) setEditing(null);
    });
  }, []);

  const active = profiles?.find((p) => p.id === settings.activeProfileId) ?? null;

  return (
    <>
      {notice && (
        <Callout
          tone="info"
          className="settings-callout"
          title="Set up a provider to use AI polish"
          action={
            <button type="button" className="icon-btn" aria-label="Dismiss" onClick={() => store.set({ polishNotice: false })}>
              <X size={14} aria-hidden />
            </button>
          }
        >
          AI polish fixes punctuation, filler words and formatting using a model you choose. Add a provider below and make it active, then turn polish on.
        </Callout>
      )}

      <Section title="AI polish" description="Optional. Off by default.">
        <div className="group">
          <Row
            label="Polish transcripts with AI"
            description={
              active
                ? `Uses ${active.label}${active.modelId ? ` · ${active.modelId}` : ""}.`
                : "Choose an active provider below to turn this on."
            }
            control={<Switch hideLabel label="Polish transcripts with AI" checked={settings.polishEnabled} disabled={!active} onChange={(v) => void setPolishEnabled(v)} />}
          />
          <div className="privacy-note">
            <ShieldCheck size={15} aria-hidden />
            <div>
              <strong>What's sent:</strong> only the text of each transcript, after it's saved locally, to your active provider. Audio never leaves this computer.{" "}
              {settings.polishEnabled ? "If the provider doesn't answer in time, the original transcript is used." : "With polish off, nothing is sent anywhere."}
            </div>
          </div>
        </div>
      </Section>

      <Section
        title="Providers"
        description="Keys are stored in your system's secure credential store, never in plain files."
        aside={
          <Menu
            label="Add provider"
            items={(Object.keys(PRESETS) as ProviderKind[]).map((k) => ({
              key: k,
              label: PRESETS[k].label,
              hint: PRESETS[k].hint,
              onSelect: () => setEditing(newProfile(k)),
            }))}
            trigger={({ open, toggle, id }) => (
              <Button size="sm" variant="secondary" icon={<Plus size={13} aria-hidden />} aria-haspopup="menu" aria-expanded={open} aria-controls={open ? id : undefined} onClick={toggle}>
                Add provider
              </Button>
            )}
          />
        }
      >
        {profiles === null ? (
          <div className="group">
            <Row label="Loading…" />
          </div>
        ) : profiles.length === 0 && !editing ? (
          <div className="providers-empty">
            <Sparkles size={18} aria-hidden />
            <div>
              <div className="row-label">No providers yet</div>
              <div className="row-desc">Add OpenRouter, a compatible API, your organization's gateway, or a model server running locally.</div>
            </div>
          </div>
        ) : (
          <div className="providers">
            {profiles.map((p) => (
              <button
                key={p.id}
                type="button"
                className={cx("provider", editing?.id === p.id && "is-editing")}
                onClick={() => setEditing(editing?.id === p.id ? null : p)}
                aria-expanded={editing?.id === p.id}
              >
                <span className="provider-icon">
                  <KindIcon kind={p.kind} />
                </span>
                <span className="provider-text">
                  <span className="provider-label">{p.label}</span>
                  <span className="provider-sub">{p.modelId || "No model selected"}</span>
                </span>
                {p.id === settings.activeProfileId ? (
                  <span className="tag tag-accent">
                    <CircleCheck size={11} aria-hidden /> Active
                  </span>
                ) : null}
                <span className="provider-edit">{editing?.id === p.id ? "Close" : "Edit"}</span>
              </button>
            ))}
          </div>
        )}

        {editing && (
          <ProfileEditor
            key={editing.id || `new-${editing.kind}`}
            initial={editing}
            isActive={editing.id !== "" && editing.id === settings.activeProfileId}
            onSaved={async (p) => {
              await reload();
              setEditing(p);
            }}
            onDeleted={async () => {
              setEditing(null);
              await reload();
            }}
            onClose={() => setEditing(null)}
          />
        )}
      </Section>
    </>
  );
}

function ProfileEditor({
  initial,
  isActive,
  onSaved,
  onDeleted,
  onClose,
}: {
  initial: ProviderProfile;
  isActive: boolean;
  onSaved: (p: ProviderProfile) => void | Promise<void>;
  onDeleted: () => void | Promise<void>;
  onClose: () => void;
}) {
  const [p, setP] = useState<ProviderProfile>(initial);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [models, setModels] = useState<ModelSummary[] | null>(null);
  const [modelsBusy, setModelsBusy] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [cred, setCred] = useState<CredentialStatus | null>(null);
  const [test, setTest] = useState<ConnectionResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const isNew = p.id === "";
  const dirty = useMemo(() => JSON.stringify(p) !== JSON.stringify(initial), [p, initial]);
  const ids = { label: useId(), base: useId(), chat: useId(), modelsPath: useId(), model: useId(), timeout: useId(), modelList: useId() };

  useEffect(() => {
    if (!isNew) void api.credentialStatus(p.id).then(setCred).catch(() => setCred(null));
  }, [isNew, p.id]);

  const set = <K extends keyof ProviderProfile>(k: K, v: ProviderProfile[K]) => setP((x) => ({ ...x, [k]: v }));

  const save = async () => {
    const problem = validateProfile(p);
    setError(problem);
    if (problem) return;
    setSaving(true);
    try {
      const saved = await api.saveProfile(p);
      toast(isNew ? "Provider added" : "Provider saved");
      setP(saved);
      await onSaved(saved);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setSaving(false);
    }
  };

  const refreshModels = async () => {
    setModelsBusy(true);
    setModelsError(null);
    try {
      setModels(await api.refreshProviderModels(p.id));
    } catch (e) {
      setModelsError(`${errorText(e)} You can still type a model ID.`);
    } finally {
      setModelsBusy(false);
    }
  };

  const runTest = async () => {
    setTesting(true);
    setTest(null);
    try {
      setTest(await api.testConnection(p.id));
    } catch (e) {
      setTest({ ok: false, message: errorText(e), latencyMs: null });
    } finally {
      setTesting(false);
    }
  };

  const headerEntries = Object.entries(p.headers);

  return (
    <div className="card editor" role="region" aria-label={isNew ? `New ${PRESETS[p.kind].label} provider` : `Edit ${initial.label}`}>
      <div className="editor-head">
        <span className="provider-icon">
          <KindIcon kind={p.kind} />
        </span>
        <div className="editor-title">
          <div className="row-label">{isNew ? `New ${PRESETS[p.kind].label}` : initial.label}</div>
          <div className="row-desc">{PRESETS[p.kind].hint}</div>
        </div>
        <button type="button" className="icon-btn" aria-label="Close editor" onClick={onClose}>
          <X size={15} aria-hidden />
        </button>
      </div>

      {cred?.needsReconnect && (
        <Callout tone="warning" title="Reconnect needed" className="editor-callout">
          The address changed since the key was saved. For your safety the old key won't be sent to the new address — save the key again to reconnect.
        </Callout>
      )}

      <div className="form-grid">
        <div className="field">
          <label className="field-label" htmlFor={ids.label}>
            Name
          </label>
          <input id={ids.label} className="input" value={p.label} onChange={(e) => set("label", e.target.value)} />
        </div>
        <div className="field">
          <label className="field-label" htmlFor={ids.timeout}>
            Timeout (seconds)
          </label>
          <input
            id={ids.timeout}
            className="input"
            type="number"
            min={1}
            max={120}
            value={Math.round(p.timeoutMs / 1000)}
            onChange={(e) => set("timeoutMs", Math.round(Number(e.target.value) * 1000))}
          />
        </div>
        <div className="field span-2">
          <label className="field-label" htmlFor={ids.base}>
            Base URL
          </label>
          <input
            id={ids.base}
            className="input mono"
            value={p.baseUrl}
            placeholder={p.kind === "local" ? "http://127.0.0.1:11434/v1" : "https://api.example.com/v1"}
            onChange={(e) => set("baseUrl", e.target.value.trim())}
            spellCheck={false}
          />
          {p.kind === "local" && <div className="field-hint">Plain http is allowed only for addresses on this computer.</div>}
        </div>
        <div className="field">
          <label className="field-label" htmlFor={ids.chat}>
            Chat path
          </label>
          <input id={ids.chat} className="input mono" value={p.chatPath} onChange={(e) => set("chatPath", e.target.value.trim())} spellCheck={false} />
        </div>
        <div className="field">
          <label className="field-label" htmlFor={ids.modelsPath}>
            Models path <span className="optional">optional</span>
          </label>
          <input
            id={ids.modelsPath}
            className="input mono"
            value={p.modelsPath ?? ""}
            placeholder="Not supported"
            onChange={(e) => set("modelsPath", e.target.value.trim() || null)}
            spellCheck={false}
          />
        </div>
        <div className="field span-2">
          <label className="field-label" htmlFor={ids.model}>
            Model ID
          </label>
          <div className="input-row">
            <input
              id={ids.model}
              className="input mono"
              value={p.modelId}
              list={models ? ids.modelList : undefined}
              placeholder="e.g. provider/model-name"
              onChange={(e) => set("modelId", e.target.value.trim())}
              spellCheck={false}
            />
            {models && (
              <datalist id={ids.modelList}>
                {models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name ?? m.id}
                  </option>
                ))}
              </datalist>
            )}
            {models && models.length > 0 && (
              <select className="select select-md model-picker" aria-label="Pick from available models" value="" onChange={(e) => e.target.value && set("modelId", e.target.value)}>
                <option value="">{models.length} available…</option>
                {models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name ? `${m.name} — ${m.id}` : m.id}
                  </option>
                ))}
              </select>
            )}
            <Button
              size="md"
              variant="secondary"
              icon={<RefreshCw size={13} aria-hidden className={modelsBusy ? "spin" : undefined} />}
              onClick={() => void refreshModels()}
              disabled={isNew || !p.modelsPath || modelsBusy}
              title={isNew ? "Save the provider first" : !p.modelsPath ? "This provider has no models path" : undefined}
            >
              Refresh models
            </Button>
          </div>
          <div className="field-hint">{modelsError ?? (isNew ? "Save first to load the model list. You can always type an ID." : "Loading the list sends no transcripts.")}</div>
        </div>
        <div className="field span-2">
          <span className="field-label">Authentication</span>
          <Segmented<AuthMode>
            label="Authentication"
            size="sm"
            value={p.authMode}
            onChange={(v) => set("authMode", v)}
            options={[
              { value: "bearer", label: "API key" },
              { value: "headers", label: "Custom headers" },
              { value: "none", label: "None" },
            ]}
          />
        </div>

        {p.authMode === "headers" && (
          <div className="field span-2">
            <span className="field-label">Secret header names</span>
            <TagListEditor
              values={p.secretHeaderNames}
              placeholder="e.g. X-Api-Key"
              onChange={(v) => set("secretHeaderNames", v)}
              label="secret header name"
            />
            <div className="field-hint">Values are saved to the secure credential store below, after you save the provider.</div>
          </div>
        )}

        <div className="field span-2">
          <span className="field-label">
            Extra headers <span className="optional">not secret</span>
          </span>
          <div className="headers">
            {headerEntries.map(([k, v], i) => (
              <div key={i} className="header-row">
                <input
                  className="input mono"
                  aria-label="Header name"
                  value={k}
                  onChange={(e) => {
                    const next = [...headerEntries];
                    next[i] = [e.target.value, v];
                    set("headers", Object.fromEntries(next));
                  }}
                />
                <input
                  className="input mono"
                  aria-label="Header value"
                  value={v}
                  onChange={(e) => {
                    const next = [...headerEntries];
                    next[i] = [k, e.target.value];
                    set("headers", Object.fromEntries(next));
                  }}
                />
                <button type="button" className="icon-btn" aria-label={`Remove header ${k}`} onClick={() => set("headers", Object.fromEntries(headerEntries.filter((_, j) => j !== i)))}>
                  <X size={14} aria-hidden />
                </button>
              </div>
            ))}
            <Button size="sm" variant="ghost" icon={<Plus size={12} aria-hidden />} onClick={() => set("headers", { ...p.headers, [`X-Header-${headerEntries.length + 1}`]: "" })}>
              Add header
            </Button>
          </div>
        </div>
      </div>

      {error && (
        <Callout tone="error" className="editor-callout" live>
          {error}
        </Callout>
      )}

      <div className="editor-actions">
        <Button variant="primary" onClick={() => void save()} loading={saving} disabled={!dirty && !isNew}>
          {isNew ? "Add provider" : "Save changes"}
        </Button>
        {!isNew && !isActive && (
          <Button variant="secondary" onClick={() => void attempt("Couldn't set active provider", () => api.setActiveProfile(p.id)).then((s) => s && (store.set({ snapshot: { ...store.get().snapshot!, settings: s } }), toast(`${p.label} is now active`)))}>
            Make active
          </Button>
        )}
        <span className="spacer" />
        {!isNew &&
          (confirmDelete ? (
            <span className="inline-confirm">
              Delete provider and its saved key?
              <Button size="sm" variant="ghost" onClick={() => setConfirmDelete(false)}>
                Keep
              </Button>
              <Button
                size="sm"
                variant="danger"
                onClick={async () => {
                  const r = await attempt("Couldn't delete provider", () => api.deleteProfile(p.id));
                  if (r !== undefined) {
                    toast("Provider deleted");
                    await onDeleted();
                  }
                }}
              >
                Delete
              </Button>
            </span>
          ) : (
            <Button variant="ghost" className="danger-ghost" icon={<Trash2 size={13} aria-hidden />} onClick={() => setConfirmDelete(true)}>
              Delete
            </Button>
          ))}
      </div>

      {!isNew && (
        <div className="editor-section">
          <div className="editor-section-title">
            <KeyRound size={14} aria-hidden /> Credentials
          </div>
          {p.authMode === "none" ? (
            <p className="row-desc">This provider doesn't use a key.</p>
          ) : (
            <div className="creds">
              {p.authMode === "bearer" && <CredentialRow profileId={p.id} name={null} label="API key" saved={!!cred?.bearerSaved} onChange={setCred} />}
              {p.authMode === "headers" &&
                (p.secretHeaderNames.length === 0 ? (
                  <p className="row-desc">Add a secret header name above, then save the provider.</p>
                ) : (
                  p.secretHeaderNames.map((n) => <CredentialRow key={n} profileId={p.id} name={n} label={n} saved={!!cred?.savedSecretHeaders.includes(n)} onChange={setCred} />)
                ))}
            </div>
          )}

          <div className="test-row">
            <Button variant="secondary" icon={<Plug size={13} aria-hidden />} loading={testing} onClick={() => void runTest()} disabled={dirty}>
              Test connection
            </Button>
            <span className="row-desc">{dirty ? "Save changes before testing." : "Sends a tiny made-up message — never your transcripts."}</span>
          </div>
          {test && (
            <div className={cx("test-result", test.ok ? "is-ok" : "is-error")} role="status">
              {test.ok ? <CircleCheck size={14} aria-hidden /> : <CircleAlert size={14} aria-hidden />}
              <span>
                {!test.ok && <strong>Couldn't connect. </strong>}
                {test.message}
                {test.latencyMs !== null && <span className="test-latency"> · {test.latencyMs} ms</span>}
              </span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function CredentialRow({ profileId, name, label, saved, onChange }: { profileId: string; name: string | null; label: string; saved: boolean; onChange: (c: CredentialStatus) => void }) {
  const [value, setValue] = useState("");
  const [replacing, setReplacing] = useState(false);
  const [busy, setBusy] = useState(false);
  const id = useId();
  const showInput = !saved || replacing;

  const save = async () => {
    if (!value) return;
    setBusy(true);
    const c = await attempt("Couldn't save key", () => api.saveCredential(profileId, name, value));
    setBusy(false);
    setValue(""); // never keep the secret around
    if (c) {
      onChange(c);
      setReplacing(false);
      toast(`${label} saved securely`);
    }
  };

  return (
    <div className="cred">
      <div className="cred-head">
        <label className="cred-label" htmlFor={id}>
          {label}
        </label>
        {saved ? (
          <span className="cred-state is-saved">
            <CircleCheck size={12} aria-hidden /> Saved
          </span>
        ) : (
          <span className="cred-state">
            <CircleAlert size={12} aria-hidden /> Not saved
          </span>
        )}
        <span className="spacer" />
        {saved && !replacing && (
          <>
            <Button size="sm" variant="ghost" onClick={() => setReplacing(true)}>
              Replace
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="danger-ghost"
              onClick={async () => {
                const c = await attempt("Couldn't remove key", () => api.deleteCredential(profileId, name));
                if (c) {
                  onChange(c);
                  toast(`${label} removed`);
                }
              }}
            >
              Remove
            </Button>
          </>
        )}
      </div>
      {showInput && (
        <form
          className="cred-form"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <input
            id={id}
            className="input mono"
            type="password"
            autoComplete="off"
            value={value}
            placeholder={saved ? "Paste the new value" : "Paste your key"}
            onChange={(e) => setValue(e.target.value)}
            spellCheck={false}
          />
          <Button variant="primary" type="submit" loading={busy} disabled={!value}>
            Save
          </Button>
          {replacing && (
            <Button
              variant="ghost"
              onClick={() => {
                setReplacing(false);
                setValue("");
              }}
            >
              Cancel
            </Button>
          )}
        </form>
      )}
    </div>
  );
}

function TagListEditor({ values, onChange, placeholder, label }: { values: string[]; onChange: (v: string[]) => void; placeholder: string; label: string }) {
  const [draft, setDraft] = useState("");
  const add = () => {
    const v = draft.trim();
    if (v && !values.includes(v)) onChange([...values, v]);
    setDraft("");
  };
  return (
    <div className="taglist">
      {values.map((v) => (
        <span key={v} className="taglist-item">
          <span className="mono">{v}</span>
          <button type="button" aria-label={`Remove ${v}`} onClick={() => onChange(values.filter((x) => x !== v))}>
            <X size={11} aria-hidden />
          </button>
        </span>
      ))}
      <input
        className="input mono taglist-input"
        aria-label={`Add ${label}`}
        value={draft}
        placeholder={placeholder}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === ",") {
            e.preventDefault();
            add();
          }
        }}
        onBlur={add}
      />
    </div>
  );
}

