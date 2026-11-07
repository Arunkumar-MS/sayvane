import { describe, expect, it } from "vitest";
import { dayLabel, formatDuration, formatMinutesSaved, formatTimer, formatWpm, groupByDay, formatCount, greeting, formatBytes } from "./format";

describe("groupByDay", () => {
  it("splits entries across local midnight into Today / Yesterday", () => {
    const now = new Date(2026, 10, 7, 0, 5); // 00:05 local
    const items = [
      { id: "a", t: new Date(2026, 10, 7, 0, 1).toISOString() }, // 4 min ago, today
      { id: "b", t: new Date(2026, 10, 6, 23, 59).toISOString() }, // 6 min ago, yesterday
      { id: "c", t: new Date(2026, 10, 6, 0, 0).toISOString() }, // yesterday, start of day
      { id: "d", t: new Date(2026, 10, 5, 23, 59).toISOString() }, // two days ago
    ];
    const groups = groupByDay(items, (i) => i.t, now, "en-US");
    expect(groups.map((g) => g.label)).toEqual(["Today", "Yesterday", "Thursday, November 5"]);
    expect(groups.map((g) => g.items.map((i) => i.id))).toEqual([["a"], ["b", "c"], ["d"]]);
  });

  it("adds the year for dates in another year", () => {
    const now = new Date(2026, 0, 2, 9);
    expect(dayLabel(new Date(2025, 11, 30, 12), now, "en-US")).toBe("Tuesday, December 30, 2025");
    expect(dayLabel(new Date(2026, 0, 1, 23, 59), now, "en-US")).toBe("Yesterday");
  });

  it("returns an empty list for no entries", () => {
    expect(groupByDay([], () => "")).toEqual([]);
  });
});

describe("stats formatting", () => {
  it("shows a dash for unknown speaking speed", () => {
    expect(formatWpm(null)).toBe("—");
    expect(formatWpm(142.6)).toBe("143");
  });
  it("labels time saved as an estimate", () => {
    expect(formatMinutesSaved(0)).toBe("~0 min");
    expect(formatMinutesSaved(12.4)).toBe("~12 min");
    expect(formatMinutesSaved(75)).toBe("~1 h 15 min");
    expect(formatMinutesSaved(120)).toBe("~2 h");
    expect(formatMinutesSaved(-5)).toBe("~0 min");
  });
  it("formats counts, durations and sizes", () => {
    expect(formatCount(12345, "en-US")).toBe("12,345");
    expect(formatDuration(9_400)).toBe("0:09");
    expect(formatDuration(252_000)).toBe("4:12");
    expect(formatDuration(3_723_000)).toBe("1:02:03");
    expect(formatTimer(83_900)).toBe("01:23");
    expect(formatBytes(147_951_465)).toBe("148 MB");
    expect(formatBytes(1_624_555_275)).toBe("1.6 GB");
  });
  it("greets by time of day", () => {
    expect(greeting(new Date(2026, 1, 1, 8))).toBe("Good morning");
    expect(greeting(new Date(2026, 1, 1, 13))).toBe("Good afternoon");
    expect(greeting(new Date(2026, 1, 1, 21))).toBe("Good evening");
  });
});
