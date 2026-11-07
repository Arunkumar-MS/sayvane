import { useRef, type KeyboardEvent } from "react";
import { Keyboard, Mic, ShieldCheck, Sparkles } from "lucide-react";
import { store, useStore, type SettingsTab } from "../../state/store";
import { isSessionActive } from "../../lib/status";
import { Callout } from "../../components/ui";
import { cx } from "../../lib/cx";
import { DictationTab } from "./DictationTab";
import { ShortcutsTab } from "./ShortcutsTab";
import { PolishTab } from "./PolishTab";
import { PrivacyTab } from "./PrivacyTab";

const TABS: Array<{ id: SettingsTab; label: string; icon: typeof Mic }> = [
  { id: "dictation", label: "Dictation", icon: Mic },
  { id: "shortcuts", label: "Shortcuts", icon: Keyboard },
  { id: "polish", label: "AI Polish", icon: Sparkles },
  { id: "privacy", label: "Permissions & Privacy", icon: ShieldCheck },
];

export function SettingsPage() {
  const tab = useStore((s) => s.settingsTab);
  const active = useStore((s) => isSessionActive(s.snapshot!.session));
  const warning = useStore((s) => s.snapshot!.settingsWarning);
  const refs = useRef<Array<HTMLButtonElement | null>>([]);

  const onKey = (e: KeyboardEvent, i: number) => {
    const dir = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const next = (i + dir + TABS.length) % TABS.length;
    store.set({ settingsTab: TABS[next].id });
    refs.current[next]?.focus();
  };

  return (
    <div className="page settings">
      <div className="tabs" role="tablist" aria-label="Settings sections">
        {TABS.map((t, i) => {
          const Icon = t.icon;
          const selected = t.id === tab;
          return (
            <button
              key={t.id}
              ref={(el) => {
                refs.current[i] = el;
              }}
              type="button"
              role="tab"
              id={`tab-${t.id}`}
              aria-selected={selected}
              aria-controls={`panel-${t.id}`}
              tabIndex={selected ? 0 : -1}
              className={cx("tab", selected && "is-active")}
              onClick={() => store.set({ settingsTab: t.id })}
              onKeyDown={(e) => onKey(e, i)}
            >
              <Icon size={14} aria-hidden />
              {t.label}
            </button>
          );
        })}
      </div>

      {warning && (
        <Callout tone="warning" className="settings-callout" title="Some settings were reset">
          {warning}
        </Callout>
      )}
      {active && (
        <Callout tone="info" className="settings-callout" live>
          A dictation is in progress. Changes apply to your next dictation.
        </Callout>
      )}

      <div className="tab-panel" role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`} key={tab}>
        {tab === "dictation" && <DictationTab />}
        {tab === "shortcuts" && <ShortcutsTab />}
        {tab === "polish" && <PolishTab />}
        {tab === "privacy" && <PrivacyTab />}
      </div>
    </div>
  );
}
