import { describe, expect, it } from "vitest";
import { resourceError } from "./resource-error";
import type { SkillMetadata } from "../types/skillsync";

const skill = (id: string, status: unknown = "up_to_date") =>
  ({ id, status }) as SkillMetadata;

describe("resource error ownership", () => {
  it("never shows FastMCP's failure on seo-audit or figma", () => {
    const errors = { fastmcp: "manifest fastmcp missing" };
    expect(resourceError(skill("seo-audit"), errors)).toBeNull();
    expect(resourceError(skill("figma"), errors)).toBeNull();
    expect(resourceError(skill("fastmcp"), errors)).toContain("fastmcp");
  });
  it("reads Rust's actual serialized error status", () => {
    expect(resourceError(skill("fastmcp", { error: "wrong source" }))).toBe(
      "wrong source",
    );
  });
  it("ignores malformed payloads and absent selection", () => {
    expect(resourceError(skill("a", { error: 42 }))).toBeNull();
    expect(resourceError(null, { a: "failure" })).toBeNull();
  });
});
