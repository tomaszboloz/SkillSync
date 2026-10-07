import type { SkillMetadata } from "../types/skillsync";

/** Rust serializes SkillStatus::Error as { error: string }, not "error". */
export function resourceError(
  skill: SkillMetadata | null,
  errors: Record<string, string> = {},
): string | null {
  if (!skill) return null;
  if (errors[skill.id]) return errors[skill.id];
  const status: unknown = skill.status;
  if (status && typeof status === "object" && "error" in status) {
    return typeof status.error === "string" ? status.error : null;
  }
  return null;
}
