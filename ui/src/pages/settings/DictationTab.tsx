import { useEffect, useId, useRef, useState } from "react";
import { Check, ChevronRight, Download, FileUp, Trash2, X, Languages, Cpu } from "lucide-react";
import { api } from "../../ipc/commands";
import type { AdvancedRecognition, AudioInput, ModelInfo } from "../../ipc/types";
import { attempt, saveSettings, toast, useStore } from "../../state/store";
import { Button, Callout, Menu, Progress, Row, Section, Segmented, Switch } from "../../components/ui";
import { formatBytes } from "../../lib/format";
import { LANGUAGES } from "../../lib/languages";
import { cx } from "../../lib/cx";

export function DictationTab() {
  return (
    <>
      <ModelsSection />
      <LanguageSection />
      <InputSection />
      <AdvancedSection />
    </>
  );
}

function ModelsSection() {
  const models = useStore((s) => s.models);
  const activeId = useStore((s) => s.snapshot!.settings.modelId);
  const notInstalled = models.filter((m) => !m.installed && !m.download);

  return (
    <Section
      title="Speech model"
      description="Models run entirely on this computer. Larger models are more accurate but slower and use more memory."
      aside={
        notInstalled.length > 0 ? (
          <Menu
            label="Import a model file for"
            items={notInstalled.map((m) => ({
              key: m.id,
              label: m.label,
              hint: `Select an existing ${formatBytes(m.sizeBytes)} model file`,
              onSelect: async () => {
                const ok = await attempt("Couldn't import model file", () => api.importModelFile(m.id));
                if (ok) toast(`${m.label} model imported`);
              },
            }))}
            trigger={({ open, toggle, id }) => (
              <Button size="sm" variant="ghost" icon={<FileUp size={13} aria-hidden />} aria-haspopup="menu" aria-expanded={open} aria-controls={open ? id : undefined} onClick={toggle}>
                Import model file
              </Button>
            )}
          />
        ) : undefined
      }
    >
      <div className="models" role="radiogroup" aria-label="Active speech model">
        {models.map((m) => (
          <ModelCard key={m.id} model={m} active={m.id === activeId} />
        ))}
        {models.length === 0 && <div className="models-empty">Loading models…</div>}
      </div>
    </Section>
  );
}

function ModelCard({ model: m, active }: { model: ModelInfo; active: boolean }) {
  const [confirmRemove, setConfirmRemove] = useState(false);
  const dl = m.download;
  const downloading = dl && dl.state !== "failed";
  const select = () => {
    if (m.installed && !active) void saveSettings({ modelId: m.id });
  };
  return (
    <div className={cx("model", active && "is-active", !m.installed && "is-missing")}>
      <div className="model-top">
        <button
          type="button"
          role="radio"
          aria-checked={active}
          aria-disabled={!m.installed || undefined}
          className="model-radio"
          onClick={select}
          aria-label={`${m.label}${m.installed ? "" : " (not installed)"}`}
        >
          <span className="radio-dot" aria-hidden>
            {active && <Check size={11} strokeWidth={3} />}
          </span>
        </button>
        <div className="model-title">
          <span className="model-label">{m.label}</span>
          {m.isDefault && <span className="tag tag-accent">Recommended</span>}
          {active && <span className="tag tag-nowrap">In use</span>}
        </div>
        <span className="model-size">{formatBytes(m.sizeBytes)}</span>
      </div>
      <p className="model-desc">{m.description}</p>
      <div className="model-tags">
        <span className="model-tag">
          <Languages size={12} aria-hidden /> {m.multilingual ? "Multilingual" : "English only"}
        </span>
        <span className="model-tag">
          <Cpu size={12} aria-hidden /> {m.recommendedMinMemoryGb} GB+ memory
        </span>
      </div>
      <div className="model-foot">
        {downloading ? (
          <div className="model-progress">
            <Progress value={dl.receivedBytes / Math.max(1, dl.totalBytes)} indeterminate={dl.state === "verifying"} label={`Downloading ${m.label}`} />
            <span className="model-progress-text">
              {dl.state === "verifying" ? "Verifying…" : `${Math.round((dl.receivedBytes / Math.max(1, dl.totalBytes)) * 100)}%`}
            </span>
            <Button size="sm" variant="ghost" icon={<X size={12} aria-hidden />} onClick={() => void attempt("Couldn't cancel", () => api.cancelModelInstall(m.id))}>
              Cancel
            </Button>
          </div>
        ) : m.installed ? (
          confirmRemove ? (
            <div className="model-confirm">
              <span>Remove from this computer?</span>
              <Button size="sm" variant="ghost" onClick={() => setConfirmRemove(false)}>
                Keep
              </Button>
              <Button
                size="sm"
                variant="danger"
                onClick={async () => {
                  await attempt("Couldn't remove model", () => api.removeModel(m.id));
                  setConfirmRemove(false);
                }}
              >
                Remove
              </Button>
            </div>
          ) : (
            <>
              <span className="model-state">
                <Check size={13} aria-hidden /> Installed
              </span>
              <span className="spacer" />
              {!active && (
                <Button size="sm" variant="secondary" onClick={select}>
                  Use this model
                </Button>
              )}
              <Button
                size="sm"
                variant="ghost"
                icon={<Trash2 size={12} aria-hidden />}
                onClick={() => setConfirmRemove(true)}
                disabled={active}
                title={active ? "Choose another model before removing this one" : undefined}
              >
                Remove
              </Button>
            </>
          )
        ) : (
          <>
            {dl?.state === "failed" && <span className="model-error">{dl.error ?? "Download failed"}</span>}
            <span className="spacer" />
            <Button size="sm" variant={m.isDefault ? "primary" : "secondary"} icon={<Download size={12} aria-hidden />} onClick={() => void attempt("Couldn't start download", () => api.installModel(m.id))}>
              {dl?.state === "failed" ? "Retry" : `Install · ${formatBytes(m.sizeBytes)}`}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}

function LanguageSection() {
  const settings = useStore((s) => s.snapshot!.settings);
  const models = useStore((s) => s.models);
  const model = models.find((m) => m.id === settings.modelId);
  const canTranslate = model ? model.canTranslate : true;
  const langId = useId();
  const hintsId = useId();
  const [hints, setHints] = useState(settings.vocabularyHints);
  useEffect(() => setHints(settings.vocabularyHints), [settings.vocabularyHints]);

  const translateConflict = settings.task === "translate" && !canTranslate;
  const englishOnly = model && !model.multilingual;

  return (
    <Section title="Language & output">
      <div className="group">
        <Row
          label="Spoken language"
          htmlFor={langId}
          description={englishOnly ? `${model.label} understands English only.` : "Auto-detect works well; pick a language if names or short phrases get misheard."}
          control={
            <select
              id={langId}
              className="select select-md"
              value={englishOnly ? "en" : (settings.language ?? "")}
              disabled={!!englishOnly}
              onChange={(e) => void saveSettings({ language: e.target.value || null })}
            >
              <option value="">Auto-detect</option>
              {LANGUAGES.map((l) => (
                <option key={l.code} value={l.code}>
                  {l.name}
                </option>
              ))}
            </select>
          }
        />
        <Row
          label="Output"
          description={
            canTranslate
              ? "Keep what you said, or translate speech in any language into English text."
              : `Translation isn't available with the ${model?.label ?? "selected"} model. Choose a multilingual model like Balanced to translate.`
          }
          control={
            <Segmented
              label="Output language"
              value={settings.task}
              onChange={(v) => void saveSettings({ task: v })}
              options={[
                { value: "transcribe", label: "Original language" },
                { value: "translate", label: "Translate to English", disabled: !canTranslate },
              ]}
            />
          }
        />
        {translateConflict && (
          <div className="row-callout">
            <Callout
              tone="warning"
              title="Translation is off for this model"
              action={
                <Button size="sm" variant="secondary" onClick={() => void saveSettings({ task: "transcribe" })}>
                  Use original language
                </Button>
              }
            >
              {model?.label} can't translate, so dictations will be transcribed in the original language.
            </Callout>
          </div>
        )}
        <div className="row row-stack">
          <div className="row-text">
            <label className="row-label" htmlFor={hintsId}>
              Vocabulary hints
            </label>
            <div className="row-desc">Names, product terms, or jargon you use often, separated by commas. Stays on this computer.</div>
            <textarea
              id={hintsId}
              className="textarea hints"
              value={hints}
              placeholder="e.g. Kubernetes, Priya Raman, Q3 roadmap"
              onChange={(e) => setHints(e.target.value)}
              onBlur={() => {
                if (hints !== settings.vocabularyHints) void saveSettings({ vocabularyHints: hints });
              }}
              rows={2}
              spellCheck={false}
            />
          </div>
        </div>
      </div>
    </Section>
  );
}

function InputSection() {
  const micId = useStore((s) => s.snapshot!.settings.microphoneId);
  const [inputs, setInputs] = useState<AudioInput[] | null>(null);
  const id = useId();
  const load = () =>
    api
      .listInputs()
      .then(setInputs)
      .catch(() => setInputs([]));
  useEffect(() => {
    void load();
  }, []);
  const def = inputs?.find((i) => i.isDefault);
  const missing = micId && inputs && !inputs.some((i) => i.id === micId);
  return (
    <Section title="Microphone">
      <div className="group">
        <Row
          label="Input device"
          htmlFor={id}
          description={
            missing
              ? "The selected microphone isn't connected. Reconnect it or choose another."
              : !micId && def
                ? `Currently ${def.name}. Follows your system choice.`
                : "Used for dictation and notes."
          }
          control={
            <>
              <select id={id} className="select select-md" value={micId ?? ""} onChange={(e) => void saveSettings({ microphoneId: e.target.value || null })}>
                <option value="">System default</option>
                {missing && <option value={micId}>Unavailable device</option>}
                {inputs?.map((i) => (
                  <option key={i.id} value={i.id}>
                    {i.name}
                  </option>
                ))}
              </select>
              <Button size="sm" variant="ghost" onClick={() => void load()}>
                Refresh
              </Button>
            </>
          }
        />
      </div>
    </Section>
  );
}

function AdvancedSection() {
  const adv = useStore((s) => s.snapshot!.settings.advanced);
  const [local, setLocal] = useState<AdvancedRecognition>(adv);
  useEffect(() => setLocal(adv), [adv]);
  const timer = useRef<number | undefined>(undefined);
  const commit = (next: AdvancedRecognition, debounce = 0) => {
    setLocal(next);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void saveSettings({ advanced: next }), debounce);
  };
  const tempId = useId();
  const nsId = useId();
  const threadsId = useId();
  const beamId = useId();

  return (
    <details className="advanced">
      <summary className="advanced-summary">
        <ChevronRight size={14} aria-hidden className="advanced-chev" />
        <span>Advanced recognition</span>
        <span className="advanced-hint">Defaults are tuned for short, fast dictation</span>
      </summary>
      <div className="group advanced-body">
        <Row
          label="Temperature"
          htmlFor={tempId}
          description="0 gives the most predictable text. Higher values may help with unusual speech."
          control={
            <span className="range-wrap">
              <input id={tempId} type="range" min={0} max={1} step={0.1} value={local.temperature} onChange={(e) => commit({ ...local, temperature: Number(e.target.value) }, 400)} />
              <output className="range-value mono">{local.temperature.toFixed(1)}</output>
            </span>
          }
        />
        <Row
          label="Decoding"
          description="Greedy is fastest. Beam search explores alternatives for slightly better accuracy."
          control={
            <>
              <Segmented
                size="sm"
                label="Decoding strategy"
                value={local.beamSize === null ? "greedy" : "beam"}
                onChange={(v) => commit({ ...local, beamSize: v === "greedy" ? null : 5 })}
                options={[
                  { value: "greedy", label: "Greedy" },
                  { value: "beam", label: "Beam" },
                ]}
              />
              {local.beamSize !== null && (
                <>
                  <label className="sr-only" htmlFor={beamId}>
                    Beam size
                  </label>
                  <select id={beamId} className="select select-sm" value={local.beamSize} onChange={(e) => commit({ ...local, beamSize: Number(e.target.value) })}>
                    {[2, 3, 4, 5, 6, 8].map((n) => (
                      <option key={n} value={n}>
                        {n} beams
                      </option>
                    ))}
                  </select>
                </>
              )}
            </>
          }
        />
        <Row label="Word timestamps" description="Finer timing for subtitle exports. Slightly slower." control={<Switch hideLabel label="Word timestamps" checked={local.wordTimestamps} onChange={(v) => commit({ ...local, wordTimestamps: v })} />} />
        <Row label="Ignore previous text" description="Treat each dictation independently. Prevents repeated phrases." control={<Switch hideLabel label="Ignore previous text" checked={local.noContext} onChange={(v) => commit({ ...local, noContext: v })} />} />
        <Row
          label="Silence sensitivity"
          htmlFor={nsId}
          description="How confident the engine must be that a segment has no speech before skipping it."
          control={
            <span className="range-wrap">
              <input id={nsId} type="range" min={0.1} max={0.9} step={0.05} value={local.noSpeechThreshold} onChange={(e) => commit({ ...local, noSpeechThreshold: Number(e.target.value) }, 400)} />
              <output className="range-value mono">{local.noSpeechThreshold.toFixed(2)}</output>
            </span>
          }
        />
        <Row
          label="Processor threads"
          htmlFor={threadsId}
          description="Auto picks a good number for this computer."
          control={
            <select id={threadsId} className="select select-sm" value={local.threads} onChange={(e) => commit({ ...local, threads: Number(e.target.value) })}>
              <option value={0}>Auto</option>
              {[1, 2, 4, 6, 8, 12].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          }
        />
      </div>
    </details>
  );
}
