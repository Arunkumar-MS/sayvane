// Pure formatting helpers (unit tested).

export function startOfLocalDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

/** Whole local calendar days between two instants (b - a). DST safe. */
export function localDayDiff(a: Date, b: Date): number {
  const da = startOfLocalDay(a);
  const db = startOfLocalDay(b);
  return Math.round((db.getTime() - da.getTime()) / 86_400_000);
}

export function dayLabel(date: Date, now: Date = new Date(), locale?: string): string {
  const diff = localDayDiff(date, now);
  if (diff === 0) return "Today";
  if (diff === 1) return "Yesterday";
  const sameYear = date.getFullYear() === now.getFullYear();
  return new Intl.DateTimeFormat(locale, {
    weekday: "long",
    month: "long",
    day: "numeric",
    ...(sameYear ? {} : { year: "numeric" }),
  }).format(date);
}

export interface DayGroup<T> {
  key: string;
  label: string;
  items: T[];
}

/** Groups items (assumed newest first) by local calendar day. */
export function groupByDay<T>(items: T[], getIso: (t: T) => string, now: Date = new Date(), locale?: string): DayGroup<T>[] {
  const groups: DayGroup<T>[] = [];
  const index = new Map<string, DayGroup<T>>();
  for (const item of items) {
    const d = new Date(getIso(item));
    const key = `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`;
    let g = index.get(key);
    if (!g) {
      g = { key, label: dayLabel(d, now, locale), items: [] };
      index.set(key, g);
      groups.push(g);
    }
    g.items.push(item);
  }
  return groups;
}

export function formatTime(iso: string, locale?: string): string {
  return new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).format(new Date(iso));
}

/** "0:09", "4:12", "1:02:03" */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** Recording timer, always mm:ss (minutes may exceed 59). */
export function formatTimer(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  return `${String(Math.floor(total / 60)).padStart(2, "0")}:${String(total % 60).padStart(2, "0")}`;
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`;
  if (bytes >= 1e3) return `${Math.round(bytes / 1e3)} KB`;
  return `${bytes} B`;
}

export function formatCount(n: number, locale?: string): string {
  return new Intl.NumberFormat(locale).format(n);
}

export function formatWpm(wpm: number | null): string {
  return wpm === null || !Number.isFinite(wpm) ? "—" : String(Math.round(wpm));
}

/** Estimated time saved, always prefixed with "~" because it is an estimate. */
export function formatMinutesSaved(minutes: number): string {
  const m = Math.max(0, Math.round(minutes));
  if (m < 60) return `~${m} min`;
  const h = Math.floor(m / 60);
  const rest = m % 60;
  return rest ? `~${h} h ${rest} min` : `~${h} h`;
}

export function formatStreak(days: number): string {
  return days === 1 ? "1 day" : `${days} days`;
}

export function greeting(now: Date = new Date()): string {
  const h = now.getHours();
  if (h < 5) return "Good evening";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}
