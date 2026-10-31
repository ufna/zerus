import { describe, expect, it } from "vitest";
import { clockOf, dayMonth, dayTimeOf, messageTimeOf, monthName } from "./dates";

const at = new Date(2025, 10, 3, 2, 7, 5);

describe("dates", () => {
  it("keeps Qt's English short months", () => {
    expect(dayMonth(at, false, "en")).toBe("3 Nov");
    expect(dayMonth(at, true, "en")).toBe("3 Nov 2025");
    expect(dayTimeOf(at, false, "en")).toBe("3 Nov 02:07");
    expect(dayTimeOf(at, true, "en")).toBe("3 Nov 02:07:05");
    expect(messageTimeOf(at, "en")).toBe("Nov 3 02:07");
  });

  it("writes Russian months in the genitive, in full and without a dot", () => {
    expect(dayMonth(at, false, "ru")).toBe("3 ноября");
    expect(dayMonth(at, true, "ru")).toBe("3 ноября 2025");
    expect(dayTimeOf(at, true, "ru")).toBe("3 ноября 02:07:05");
    expect(messageTimeOf(at, "ru")).toBe("3 ноября 02:07");
    const months = Array.from({ length: 12 }, (_, m) => monthName(new Date(2026, m, 3), "ru"));
    expect(months).toEqual(["января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября", "октября", "ноября", "декабря"]);
    expect(months.some((name) => name.includes("."))).toBe(false);
  });

  it("uses a 24-hour clock", () => {
    expect(clockOf(new Date(2025, 0, 1, 23, 5, 9))).toBe("23:05");
    expect(clockOf(new Date(2025, 0, 1, 23, 5, 9), true)).toBe("23:05:09");
  });
});
