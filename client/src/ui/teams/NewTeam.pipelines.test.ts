// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";
import { pipelineLines } from "./NewTeam";
import { catalog } from "./fixtures";

// The template summary shows the pipelines the template resolves to (its
// default and its seated roles' role defaults), not every catalog pipeline,
// and marks the default.
describe("the template's pipelines", () => {
  const extra = { id: "unrelated", name: "Unrelated", implementer: "developer", stages: [], max_rounds: 0, accept: "lead" };
  const roleDefault = { id: "developer-default", name: "Developer default", implementer: "developer", stages: [], max_rounds: 0, accept: "lead" };
  const roles = catalog.roles.map((role) => role.id === "developer" ? { ...role, pipeline: roleDefault.id } : role);
  const template = { ...catalog.templates[0], default_pipeline: "reviewed", roster: [{ role: "developer", count: 1, optional: false }] };

  it("lists only the template's pipelines and marks the default", () => {
    const lines = pipelineLines({ ...catalog, roles, pipelines: [...catalog.pipelines, extra, roleDefault] }, template);
    expect(lines.some((line) => line.includes("Unrelated"))).toBe(false);
    expect(lines.find((line) => line.includes("Reviewed"))).toContain("(default)");
    expect(lines.some((line) => line.includes("Developer default"))).toBe(true);
  });

  it("does not count a seat with no members", () => {
    const lines = pipelineLines({ ...catalog, roles, pipelines: [...catalog.pipelines, extra, roleDefault] }, { ...template, roster: [{ role: "developer", count: 0, optional: false }] });
    expect(lines.some((line) => line.includes("Developer default"))).toBe(false);
  });
});
