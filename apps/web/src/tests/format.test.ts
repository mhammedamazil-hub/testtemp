import { describe, expect, it } from "vitest";
import {
  estimateSeconds,
  formatBytes,
  formatCount,
  formatDuration,
  progressPercent,
  relativeTime,
} from "../lib/format";

describe("formatBytes", () => {
  it("handles empty and zero values", () => {
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(0)).toBe("0 B");
  });
  it("uses binary units", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(60 * 1024 * 1024)).toBe("60.0 MB");
    expect(formatBytes(2 * 1024 ** 3, 0)).toBe("2 GB");
  });
});

describe("formatCount", () => {
  it("abbreviates large counts", () => {
    expect(formatCount(262_144)).toBe("262.1K");
    expect(formatCount(1_048_576)).toBe("1.05M");
    expect(formatCount(500)).toBe("500");
  });
});

describe("formatDuration", () => {
  it("scales units", () => {
    expect(formatDuration(420)).toBe("420 ms");
    expect(formatDuration(28_400)).toBe("28 s");
    expect(formatDuration(125_000)).toBe("2m 05s");
  });
});

describe("relativeTime", () => {
  const now = Date.parse("2026-01-01T12:00:00Z");
  it("describes recent jobs", () => {
    expect(relativeTime("2026-01-01T11:59:40Z", now)).toBe("just now");
    expect(relativeTime("2026-01-01T11:58:00Z", now)).toBe("2 min ago");
  });
});

describe("progress helpers", () => {
  it("clamps progress", () => {
    expect(progressPercent(0.42)).toBe(42);
    expect(progressPercent(1.4)).toBe(100);
    expect(progressPercent(-0.2)).toBe(0);
  });
  it("estimates Idol timings from its published numbers", () => {
    expect(estimateSeconds(262_144, 50)).toContain("RTX 4090");
    expect(estimateSeconds(65_536, 30)).toContain("~10 s");
  });
});
