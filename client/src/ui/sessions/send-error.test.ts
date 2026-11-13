import { expect, it, vi } from "vitest";
vi.mock("../../i18n", () => ({ trNoop: (text: string) => text, tr: () => "Подтверждённая причина NO_COLOR" }));
import { NO_COLOR_SEND, sendErrorText } from "./send-error";

it("localizes the verified refusal inside a failed row while retaining its receipt diagnostic", () => {
  const caution = "Delivery not confirmed. Check Terminal before retrying.";
  expect(sendErrorText(`${caution}\nhgs send (1): hgs: ${NO_COLOR_SEND}`))
    .toBe(`${caution}\nhgs send (1): hgs: Подтверждённая причина NO_COLOR`);
  expect(sendErrorText("hgs: generic draft refusal")).toBe("hgs: generic draft refusal");
});
