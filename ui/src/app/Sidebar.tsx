import { History, House, Settings } from "lucide-react";
import type { ReactNode } from "react";
import { navigate, useStore } from "../state/store";
import { LogoMark } from "../components/Logo";
import { Keycaps } from "../components/ui";
import { cx } from "../lib/cx";
import type { Page } from "../ipc/types";
import { formatCount } from "../lib/format";

export function Sidebar() {
  const page = useStore((s) => s.page);
  const brand = useStore((s) => s.snapshot!.brand);
  const count = useStore((s) => s.stats?.totalEntries ?? s.snapshot!.historyCount);

  return (
    <aside className="sidebar" aria-label="Sidebar">
      <div className="brand" data-tauri-drag-region>
        <LogoMark size={30} />
        <div className="brand-text">
          <span className="brand-name">{brand.displayName}</span>
          <span className="brand-tagline">{brand.tagline}</span>
        </div>
      </div>

      <nav className="nav" aria-label="Main">
        <NavItem page="home" current={page} icon={<House size={16} aria-hidden />} label="Home" />
        <NavItem
          page="history"
          current={page}
          icon={<History size={16} aria-hidden />}
          label="History"
          badge={count > 0 ? formatCount(count) : undefined}
          badgeLabel={`${count} entries`}
        />
        <NavItem page="settings" current={page} icon={<Settings size={16} aria-hidden />} label="Settings" />
      </nav>

      <div className="sidebar-foot">
        <ShortcutHint />
        <EngineStatusLine />
      </div>
    </aside>
  );
}

function NavItem({ page, current, icon, label, badge, badgeLabel }: { page: Page; current: Page; icon: ReactNode; label: string; badge?: string; badgeLabel?: string }) {
  const active = page === current;
  return (
    <button type="button" className={cx("nav-item", active && "is-active")} aria-current={active ? "page" : undefined} onClick={() => navigate(page)}>
      {icon}
      <span className="nav-label">{label}</span>
      {badge && (
        <span className="nav-badge" aria-label={badgeLabel}>
          {badge}
        </span>
      )}
    </button>
  );
}

export function useDictationShortcut(): { verb: string; accelerator: string } {
  const os = useStore((s) => s.snapshot!.platform.os);
  const fn = useStore((s) => s.snapshot!.settings.fnGestureEnabled);
  const ptt = useStore((s) => s.snapshot!.settings.shortcuts.find((b) => b.action === "pushToTalk")?.accelerator ?? "Control+Alt+Space");
  if (os === "macos" && fn) return { verb: "Hold", accelerator: "Fn" };
  return { verb: "Hold", accelerator: ptt };
}

function ShortcutHint() {
  const os = useStore((s) => s.snapshot!.platform.os);
  const { verb, accelerator } = useDictationShortcut();
  return (
    <div className="shortcut-hint">
      <span className="shortcut-hint-label">{verb} to dictate</span>
      <Keycaps accelerator={accelerator} os={os} size="sm" />
    </div>
  );
}

function EngineStatusLine() {
  const engine = useStore((s) => s.snapshot!.engine);
  const { label, tone } =
    engine.state === "ready" || engine.state === "cold"
      ? { label: "Ready", tone: "ok" }
      : engine.state === "busy"
        ? { label: "Working", tone: "ok" }
        : engine.state === "loading"
          ? { label: "Loading model", tone: "busy" }
          : engine.state === "noModel"
            ? { label: "No model installed", tone: "warn" }
            : { label: "Engine error", tone: "err" };
  return (
    <div className="engine-line" role="status" aria-live="polite" title={engine.message ?? undefined}>
      <span className={cx("engine-dot", `is-${tone}`)} aria-hidden />
      <span className="engine-text">
        <span className="engine-caption">Local speech engine</span>
        <span className="engine-state">{label}</span>
      </span>
    </div>
  );
}
