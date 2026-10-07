import { beforeEach, describe, expect, it, vi } from "vitest";
import { useSkillStore } from "./useSkillStore";
import { api } from "../lib/ipc";
import { resourceError } from "../lib/resource-error";
import type { SkillMetadata } from "../types/skillsync";

vi.mock("../lib/ipc", () => ({
  api: { updateSingleSkill: vi.fn(), checkGitHubUpdate: vi.fn() },
}));
const skill = (id: string) =>
  ({
    id,
    name: id,
    status: "update_available",
    updateAvailable: true,
  }) as SkillMetadata;

describe("update failure ownership", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useSkillStore.setState({
      skills: [skill("fastmcp"), skill("seo-audit"), skill("figma")],
      error: null,
      resourceErrors: {},
    });
  });
  it("keeps an update error on its actual resource when opening another modal", async () => {
    vi.mocked(api.updateSingleSkill).mockRejectedValueOnce(
      "fastmcp manifest missing",
    );
    await useSkillStore.getState().updateSkill("fastmcp");
    for (const id of ["seo-audit", "figma"]) {
      useSkillStore.getState().openDetail(skill(id));
      const state = useSkillStore.getState();
      expect(
        resourceError(state.selectedSkill, state.resourceErrors),
      ).toBeNull();
    }
    expect(
      resourceError(skill("fastmcp"), useSkillStore.getState().resourceErrors),
    ).toContain("fastmcp");
  });
  it("clears the matching failure when a retry succeeds", async () => {
    useSkillStore.setState({
      resourceErrors: { fastmcp: "old failure", figma: "other failure" },
    });
    vi.mocked(api.updateSingleSkill).mockResolvedValueOnce({
      ...skill("fastmcp"),
      status: "up_to_date",
      updateAvailable: false,
    });
    await useSkillStore.getState().updateSkill("fastmcp");
    expect(useSkillStore.getState().resourceErrors).toEqual({
      figma: "other failure",
    });
  });
  it("clears the matching failure after a fresh successful check", async () => {
    useSkillStore.setState({ resourceErrors: { fastmcp: "old failure" } });
    vi.mocked(api.checkGitHubUpdate).mockResolvedValueOnce(skill("fastmcp"));
    await useSkillStore.getState().checkGitHubUpdate("fastmcp");
    expect(useSkillStore.getState().resourceErrors).toEqual({});
  });
});
