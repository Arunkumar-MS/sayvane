// Shared primitives. Every state uses icon + text, never color alone.
import {
  useEffect,
  useId,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type ReactNode,
} from "react";
import {
  Ban,
  CircleAlert,
  CircleCheck,
  Clock,
  Info,
  LoaderCircle,
  Mic,
  MicOff,
  Save,
  Send,
  Sparkles,
  TriangleAlert,
  X,
  CircleX,
} from "lucide-react";
import { cx } from "../lib/cx";
import type { StatusIcon, StatusPill, Tone } from "../lib/status";
import { acceleratorKeycaps, acceleratorSpoken } from "../lib/accelerator";
import type { Os } from "../ipc/types";
import { dismissToast, useStore } from "../state/store";

// ---------- Button ----------

type Variant = "primary" | "secondary" | "ghost" | "danger" | "subtle";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: "sm" | "md";
  icon?: ReactNode;
  loading?: boolean;
}

export function Button({ variant = "secondary", size = "md", icon, loading, className, children, disabled, ...rest }: ButtonProps) {
  return (
    <button
      type="button"
      className={cx("btn", `btn-${variant}`, `btn-${size}`, loading && "is-loading", className)}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      {...rest}
    >
      {loading ? <LoaderCircle className="spin" size={size === "sm" ? 13 : 15} aria-hidden /> : icon}
      {children !== undefined && <span className="btn-label">{children}</span>}
    </button>
  );
}

// ---------- Switch ----------

export function Switch({
  checked,
  onChange,
  label,
  disabled,
  describedBy,
  hideLabel,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  disabled?: boolean;
  describedBy?: string;
  hideLabel?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={hideLabel ? label : undefined}
      aria-describedby={describedBy}
      disabled={disabled}
      className={cx("switch", checked && "is-on")}
      onClick={() => onChange(!checked)}
    >
      <span className="switch-track" aria-hidden>
        <span className="switch-thumb" />
      </span>
      {!hideLabel && <span className="switch-label">{label}</span>}
    </button>
  );
}

// ---------- Tooltip ----------

export function Tooltip({ text, children, side = "top", className }: { text: ReactNode; children: ReactNode; side?: "top" | "bottom"; className?: string }) {
  const id = useId();
  return (
    <span className={cx("tip", `tip-${side}`, className)} aria-describedby={id}>
      {children}
      <span role="tooltip" id={id} className="tip-bubble">
        {text}
      </span>
    </span>
  );
}

// ---------- Keycaps ----------

export function Keycaps({ accelerator, os, size = "md" }: { accelerator: string; os: Os; size?: "sm" | "md" }) {
  const caps = acceleratorKeycaps(accelerator, os);
  return (
    <span className={cx("keycaps", `keycaps-${size}`)} aria-label={acceleratorSpoken(accelerator, os)} role="img">
      {caps.map((c, i) => (
        <kbd key={i} className={cx("keycap", c.length > 1 && "is-wide")} aria-hidden>
          {c}
        </kbd>
      ))}
    </span>
  );
}

// ---------- Status ----------

export function StatusGlyph({ icon, size = 13 }: { icon: StatusIcon; size?: number }) {
  const p = { size, "aria-hidden": true, strokeWidth: 2.2 } as const;
  switch (icon) {
    case "check":
      return <CircleCheck {...p} />;
    case "send":
      return <Send {...p} />;
    case "save":
      return <Save {...p} />;
    case "alert":
      return <TriangleAlert {...p} />;
    case "x":
      return <CircleX {...p} />;
    case "info":
      return <Info {...p} />;
    case "spinner":
      return <LoaderCircle {...p} className="spin" />;
    case "sparkles":
      return <Sparkles {...p} />;
    case "ban":
      return <Ban {...p} />;
    case "mic":
      return <Mic {...p} />;
    case "clock":
      return <Clock {...p} />;
    case "mute":
      return <MicOff {...p} />;
  }
}

export function Pill({ pill, className }: { pill: StatusPill; className?: string }) {
  const body = (
    <span className={cx("pill", `tone-${pill.tone}`, className)}>
      <StatusGlyph icon={pill.icon} size={12} />
      <span>{pill.label}</span>
      {pill.hint && <span className="sr-only">. {pill.hint}</span>}
    </span>
  );
  return pill.hint ? <Tooltip text={pill.hint}>{body}</Tooltip> : body;
}

// ---------- Callout / banner ----------

export function Callout({
  tone = "info",
  title,
  children,
  action,
  icon,
  className,
  live,
}: {
  tone?: Tone;
  title?: ReactNode;
  children?: ReactNode;
  action?: ReactNode;
  icon?: ReactNode;
  className?: string;
  live?: boolean;
}) {
  const glyph =
    icon ??
    (tone === "error" ? <CircleAlert size={16} aria-hidden /> : tone === "warning" ? <TriangleAlert size={16} aria-hidden /> : tone === "success" ? <CircleCheck size={16} aria-hidden /> : <Info size={16} aria-hidden />);
  return (
    <div className={cx("callout", `tone-${tone}`, className)} role={live ? (tone === "error" ? "alert" : "status") : undefined}>
      <span className="callout-icon">{glyph}</span>
      <div className="callout-body">
        {title && <div className="callout-title">{title}</div>}
        {children && <div className="callout-text">{children}</div>}
      </div>
      {action && <div className="callout-action">{action}</div>}
    </div>
  );
}

// ---------- Progress ----------

export function Progress({ value, label, indeterminate }: { value: number; label: string; indeterminate?: boolean }) {
  const pct = Math.round(Math.max(0, Math.min(1, value)) * 100);
  return (
    <div
      className={cx("progress", indeterminate && "is-indeterminate")}
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={indeterminate ? undefined : pct}
    >
      <span className="progress-fill" style={{ width: indeterminate ? undefined : `${pct}%` }} />
    </div>
  );
}

// ---------- Segmented control ----------

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
  size = "md",
}: {
  value: T;
  options: Array<{ value: T; label: string; disabled?: boolean; icon?: ReactNode }>;
  onChange: (v: T) => void;
  label: string;
  size?: "sm" | "md";
}) {
  const refs = useRef<Array<HTMLButtonElement | null>>([]);
  const enabled = options.filter((o) => !o.disabled);
  const move = (dir: 1 | -1) => {
    const i = enabled.findIndex((o) => o.value === value);
    const next = enabled[(i + dir + enabled.length) % enabled.length];
    onChange(next.value);
    refs.current[options.indexOf(next)]?.focus();
  };
  return (
    <div className={cx("segmented", `segmented-${size}`)} role="radiogroup" aria-label={label}>
      {options.map((o, i) => (
        <button
          key={o.value}
          ref={(el) => {
            refs.current[i] = el;
          }}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          tabIndex={o.value === value ? 0 : -1}
          disabled={o.disabled}
          className={cx("segmented-item", o.value === value && "is-active")}
          onClick={() => onChange(o.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowRight" || e.key === "ArrowDown") {
              e.preventDefault();
              move(1);
            } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
              e.preventDefault();
              move(-1);
            }
          }}
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  );
}

// ---------- Menu ----------

export interface MenuItem {
  key: string;
  label: string;
  hint?: string;
  disabled?: boolean;
  disabledReason?: string;
  onSelect: () => void;
}

export function Menu({ trigger, items, label, align = "end" }: { trigger: (p: { open: boolean; toggle: () => void; id: string }) => ReactNode; items: MenuItem[]; label: string; align?: "start" | "end" }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const wrap = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);

  useEffect(() => {
    if (!open) return;
    const first = items.findIndex((i) => !i.disabled);
    itemRefs.current[first]?.focus();
    const onDoc = (e: MouseEvent) => {
      if (!wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open, items]);

  const focusAt = (dir: 1 | -1) => {
    const els = itemRefs.current.filter((e): e is HTMLButtonElement => !!e && !e.disabled);
    const i = els.indexOf(document.activeElement as HTMLButtonElement);
    els[(i + dir + els.length) % els.length]?.focus();
  };

  return (
    <div
      className="menu-wrap"
      ref={wrap}
      onKeyDown={(e) => {
        if (!open) return;
        if (e.key === "Escape") {
          e.stopPropagation();
          setOpen(false);
          (wrap.current?.querySelector("[aria-haspopup]") as HTMLElement | null)?.focus();
        } else if (e.key === "ArrowDown") {
          e.preventDefault();
          focusAt(1);
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          focusAt(-1);
        } else if (e.key === "Tab") setOpen(false);
      }}
    >
      {trigger({ open, toggle: () => setOpen((o) => !o), id })}
      {open && (
        <div className={cx("menu", `menu-${align}`)} role="menu" id={id} aria-label={label}>
          {items.map((item, i) => (
            <button
              key={item.key}
              ref={(el) => {
                itemRefs.current[i] = el;
              }}
              type="button"
              role="menuitem"
              className="menu-item"
              aria-disabled={item.disabled || undefined}
              disabled={item.disabled}
              title={item.disabled ? item.disabledReason : undefined}
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
            >
              <span className="menu-item-label">{item.label}</span>
              {(item.disabled ? item.disabledReason : item.hint) && (
                <span className="menu-item-hint">{item.disabled ? item.disabledReason : item.hint}</span>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

// ---------- Toasts ----------

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  return (
    <div className="toasts" aria-live="polite" aria-atomic="false">
      {toasts.map((t) => (
        <div key={t.id} className={cx("toast", `toast-${t.tone}`)} role={t.tone === "error" ? "alert" : "status"}>
          <span className="toast-icon">
            {t.tone === "error" ? <CircleAlert size={15} aria-hidden /> : t.tone === "info" ? <Info size={15} aria-hidden /> : <CircleCheck size={15} aria-hidden />}
          </span>
          <span className="toast-text">
            <span className="toast-title">{t.message}</span>
            {t.detail && <span className="toast-detail">{t.detail}</span>}
          </span>
          <button type="button" className="toast-close" aria-label="Dismiss" onClick={() => dismissToast(t.id)}>
            <X size={13} aria-hidden />
          </button>
        </div>
      ))}
    </div>
  );
}

// ---------- Settings layout helpers ----------

export function Section({ title, description, children, aside }: { title: string; description?: ReactNode; children: ReactNode; aside?: ReactNode }) {
  const id = useId();
  return (
    <section className="section" aria-labelledby={id}>
      <div className="section-head">
        <div>
          <h2 className="section-title" id={id}>
            {title}
          </h2>
          {description && <p className="section-desc">{description}</p>}
        </div>
        {aside}
      </div>
      {children}
    </section>
  );
}

export function Row({ label, description, control, htmlFor, children, className }: { label: ReactNode; description?: ReactNode; control?: ReactNode; htmlFor?: string; children?: ReactNode; className?: string }) {
  return (
    <div className={cx("row", className)}>
      <div className="row-text">
        {htmlFor ? (
          <label className="row-label" htmlFor={htmlFor}>
            {label}
          </label>
        ) : (
          <div className="row-label">{label}</div>
        )}
        {description && <div className="row-desc">{description}</div>}
        {children}
      </div>
      {control && <div className="row-control">{control}</div>}
    </div>
  );
}
