// @vitest-environment happy-dom
// Task #427's strip thresholds: the fork's compactFor, kept for the team bar.
import { expect, it } from "vitest";
import { compactFor } from "./team-bar";

it("folds the strip by its own width", () => {
  expect(compactFor(1300)).toBe("none");
  expect(compactFor(1100)).toBe("none");
  expect(compactFor(1099)).toBe("low");
  expect(compactFor(900)).toBe("low");
  expect(compactFor(899)).toBe("high");
  expect(compactFor(320)).toBe("high");
});
