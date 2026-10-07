import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import {
  SkillMetadata,
  AppConfig,
  AppUpdateProgress,
  ManagedItemType,
  AppUpdateInfo,
} from "../types/skillsync";
import { api } from "../lib/ipc";
import { Language } from "../i18n/types";

interface DirtyUpdateConfirmation {
  skillId: string;
  skillName: string;
  reason: string;
}

export const formatError = (err: unknown, fallback: string): string => {
  if (typeof err === "string" && err.trim().length > 0) return err;
  if (err instanceof Error) return err.message;
  if (err && typeof err === "object" && "message" in err) {
    const message = (err as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return fallback;
};

interface SkillState {
  skills: SkillMetadata[];
  isLoading: boolean;
  isScanning: boolean;
  error: string | null;
  resourceErrors: Record<string, string>;
  searchQuery: string;
  selectedScope: string;
  statusFilter: "all" | "updates" | "up_to_date";
  itemTypeFilter: "all" | ManagedItemType;
  branchFilter: "all" | "missing";
  queuedChecks: number;
  selectedSkill: SkillMetadata | null;
  isDetailOpen: boolean;
  isSettingsOpen: boolean;
  isUpdateCenterOpen: boolean;
  config: AppConfig | null;
  theme: "dark" | "light";
  language: Language;
  batchUpdating: boolean;
  appUpdate: AppUpdateInfo | null;
  isCheckingAppUpdate: boolean;
  appUpdateError: string | null;
  appUpdateProgress: AppUpdateProgress;
  pendingDirtyUpdate: DirtyUpdateConfirmation | null;
  deletingSkillId: string | null;
  batchProgress: {
    total: number;
    completed: number;
    currentItem?: string;
  };

  // Actions
  fetchSkills: (forceRefresh?: boolean) => Promise<void>;
  setSearchQuery: (query: string) => void;
  setSelectedScope: (scope: string) => void;
  setStatusFilter: (filter: "all" | "updates" | "up_to_date") => void;
  setItemTypeFilter: (filter: "all" | ManagedItemType) => void;
  setBranchFilter: (filter: "all" | "missing") => void;
  applyScanResult: (skill: SkillMetadata) => void;
  openDetail: (skill: SkillMetadata) => void;
  closeDetail: () => void;
  openSettings: () => void;
  closeSettings: () => void;
  openUpdateCenter: () => void;
  closeUpdateCenter: () => void;
  updateSkill: (skillId: string, force?: boolean) => Promise<void>;
  confirmDirtyUpdate: () => Promise<void>;
  dismissDirtyUpdate: () => void;
  checkGitHubUpdate: (skillId: string) => Promise<void>;
  checkoutCustomVersion: (skillId: string, targetRef: string) => Promise<void>;
  setBranchOverride: (skillId: string, branch: string | null) => Promise<void>;
  setRepositoryOverride: (
    skillId: string,
    repository: string | null,
  ) => Promise<void>;
  removeSkill: (skillId: string, locations: string[]) => Promise<void>;
  batchUpdateAll: () => Promise<void>;
  rollbackSkill: (skillId: string, snapshotId?: string) => Promise<void>;
  setTheme: (theme: "dark" | "light") => void;
  toggleTheme: () => void;
  setLanguage: (lang: Language) => void;
  loadConfig: () => Promise<void>;
  saveConfig: (config: AppConfig) => Promise<void>;
  checkAppUpdate: () => Promise<void>;
  installAppUpdate: () => Promise<void>;
}

export const useSkillStore = create<SkillState>()(
  immer((set, get) => ({
    skills: [],
    isLoading: true,
    isScanning: false,
    error: null,
    resourceErrors: {},
    searchQuery: "",
    selectedScope: "all",
    statusFilter: "all",
    itemTypeFilter: "all",
    branchFilter: "all",
    queuedChecks: 0,
    selectedSkill: null,
    isDetailOpen: false,
    isSettingsOpen: false,
    isUpdateCenterOpen: false,
    config: null,
    theme: "dark",
    language:
      (typeof window !== "undefined" &&
        (localStorage.getItem("skillsync_lang") as Language)) ||
      "pl",
    batchUpdating: false,
    appUpdate: null,
    isCheckingAppUpdate: false,
    appUpdateError: null,
    appUpdateProgress: {
      phase: "idle",
      downloadedBytes: 0,
      contentLength: null,
    },
    pendingDirtyUpdate: null,
    deletingSkillId: null,
    batchProgress: {
      total: 0,
      completed: 0,
    },

    fetchSkills: async (forceRefresh = false) => {
      set((state) => {
        state.isScanning = true;
        state.error = null;
      });
      try {
        const skills = await api.scanSkills(forceRefresh);
        set((state) => {
          state.skills = skills;
          state.resourceErrors = {};
          if (state.selectedSkill) {
            state.selectedSkill =
              skills.find((skill) => skill.id === state.selectedSkill?.id) ??
              state.selectedSkill;
          }
          state.queuedChecks = skills.filter(
            (skill) => skill.status === "checking",
          ).length;
          state.isLoading = false;
          state.isScanning = false;
        });
      } catch (err: unknown) {
        set((state) => {
          state.error = formatError(err, "Failed to scan skills");
          state.isLoading = false;
          state.isScanning = false;
        });
      }
    },

    setSearchQuery: (query: string) => {
      set((state) => {
        state.searchQuery = query;
      });
    },

    setSelectedScope: (scope: string) => {
      set((state) => {
        state.selectedScope = scope;
      });
    },

    setStatusFilter: (filter: "all" | "updates" | "up_to_date") => {
      set((state) => {
        state.statusFilter = filter;
      });
    },

    setItemTypeFilter: (filter: "all" | ManagedItemType) => {
      set((state) => {
        state.itemTypeFilter = filter;
      });
    },

    setBranchFilter: (filter: "all" | "missing") => {
      set((state) => {
        state.branchFilter = filter;
      });
    },

    applyScanResult: (skill: SkillMetadata) => {
      set((state) => {
        delete state.resourceErrors[skill.id];
        const index = state.skills.findIndex((item) => item.id === skill.id);
        if (index !== -1) state.skills[index] = skill;
        if (state.selectedSkill?.id === skill.id) state.selectedSkill = skill;
        state.queuedChecks = state.skills.filter(
          (item) => item.status === "checking",
        ).length;
      });
    },

    openDetail: (skill: SkillMetadata) => {
      set((state) => {
        state.selectedSkill = skill;
        state.isDetailOpen = true;
      });
    },

    closeDetail: () => {
      set((state) => {
        state.isDetailOpen = false;
        state.selectedSkill = null;
      });
    },

    openSettings: () => {
      set((state) => {
        state.isSettingsOpen = true;
      });
    },

    closeSettings: () => {
      set((state) => {
        state.isSettingsOpen = false;
      });
    },

    openUpdateCenter: () => {
      set((state) => {
        state.isUpdateCenterOpen = true;
      });
    },

    closeUpdateCenter: () => {
      set((state) => {
        state.isUpdateCenterOpen = false;
      });
    },

    updateSkill: async (skillId: string, force = false) => {
      set((state) => {
        delete state.resourceErrors[skillId];
        const idx = state.skills.findIndex((s) => s.id === skillId);
        if (idx !== -1) {
          state.skills[idx].status = "updating";
        }
        if (state.selectedSkill?.id === skillId) {
          state.selectedSkill.status = "updating";
        }
      });

      try {
        const updated = await api.updateSingleSkill(skillId, undefined, force);
        set((state) => {
          const idx = state.skills.findIndex((s) => s.id === skillId);
          if (idx !== -1) {
            state.skills[idx] = updated;
          }
          if (state.selectedSkill?.id === skillId) {
            state.selectedSkill = updated;
          }
        });
      } catch (err: unknown) {
        const message = formatError(err, "Update failed");
        const requiresConfirmation =
          !force &&
          message.includes("Katalog roboczy zawiera niezacommitowane zmiany");
        set((state) => {
          const idx = state.skills.findIndex((s) => s.id === skillId);
          if (idx !== -1) {
            state.skills[idx].status = "error";
          }
          if (state.selectedSkill?.id === skillId) {
            state.selectedSkill.status = "error";
          }
          state.error = requiresConfirmation
            ? `Aktualizacja wymaga decyzji: ${message}`
            : message;
          state.resourceErrors[skillId] = state.error;
          if (requiresConfirmation) {
            const skill = state.skills.find((item) => item.id === skillId);
            state.pendingDirtyUpdate = {
              skillId,
              skillName: skill?.name ?? skillId,
              reason: message,
            };
          }
        });
      }
    },

    confirmDirtyUpdate: async () => {
      const pending = get().pendingDirtyUpdate;
      if (!pending) return;
      set((state) => {
        state.pendingDirtyUpdate = null;
        state.error = null;
      });
      await get().updateSkill(pending.skillId, true);
    },

    dismissDirtyUpdate: () => {
      set((state) => {
        state.pendingDirtyUpdate = null;
      });
    },

    checkGitHubUpdate: async (skillId: string) => {
      try {
        const updated = await api.checkGitHubUpdate(skillId);
        set((state) => {
          delete state.resourceErrors[skillId];
          const idx = state.skills.findIndex((s) => s.id === skillId);
          if (idx !== -1) {
            state.skills[idx] = updated;
          }
          if (state.selectedSkill?.id === skillId) {
            state.selectedSkill = updated;
          }
        });
      } catch (err: unknown) {
        set((state) => {
          state.error = formatError(err, "GitHub check failed");
          state.resourceErrors[skillId] = state.error;
        });
      }
    },

    checkoutCustomVersion: async (skillId: string, targetRef: string) => {
      set((state) => {
        delete state.resourceErrors[skillId];
        const idx = state.skills.findIndex((s) => s.id === skillId);
        if (idx !== -1) {
          state.skills[idx].status = "updating";
        }
      });

      try {
        const updated = await api.checkoutCustomVersion(skillId, targetRef);
        set((state) => {
          const idx = state.skills.findIndex((s) => s.id === skillId);
          if (idx !== -1) {
            state.skills[idx] = updated;
          }
          if (state.selectedSkill?.id === skillId) {
            state.selectedSkill = updated;
          }
        });
      } catch (err: unknown) {
        set((state) => {
          const idx = state.skills.findIndex((s) => s.id === skillId);
          if (idx !== -1) {
            state.skills[idx].status = "error";
          }
          state.error = formatError(err, "Checkout failed");
          state.resourceErrors[skillId] = state.error;
        });
      }
    },

    setBranchOverride: async (skillId: string, branch: string | null) => {
      try {
        const normalizedBranch = branch?.trim() || null;
        await api.setBranchOverride(skillId, normalizedBranch);

        // The counter and the "Without branch" view must react to a manual
        // assignment immediately, rather than wait for a full rescan or for a
        // remote request to finish. The following remote check replaces this
        // optimistic value with the authoritative backend result.
        set((state) => {
          const updateBranch = (skill: SkillMetadata) => {
            if (skill.id !== skillId) return;
            skill.branchOverride = normalizedBranch;
            if (normalizedBranch) skill.branchOrTag = normalizedBranch;
          };
          state.skills.forEach(updateBranch);
          if (state.selectedSkill) updateBranch(state.selectedSkill);
        });

        // Clearing an override needs a scan to restore the branch that Git
        // detects automatically. Assigning one has already updated the UI.
        if (!normalizedBranch) await get().fetchSkills(true);
        if (
          get().skills.some((skill) => skill.id === skillId && skill.remoteUrl)
        ) {
          await get().checkGitHubUpdate(skillId);
        }
      } catch (err: unknown) {
        set((state) => {
          state.error = formatError(
            err,
            "Nie udało się zapisać gałęzi śledzenia",
          );
          state.resourceErrors[skillId] = state.error;
        });
      }
    },

    removeSkill: async (skillId: string, locations: string[]) => {
      if (locations.length === 0) return;
      set((state) => {
        state.deletingSkillId = skillId;
        state.error = null;
      });
      try {
        await api.removeSkill(skillId, locations);
        await get().fetchSkills(true);
        if (!get().skills.some((skill) => skill.id === skillId)) {
          get().closeDetail();
        }
      } catch (err: unknown) {
        // A remove-everywhere request can now complete some locations even if
        // another is blocked by a damaged manifest or backup error. Refresh
        // discovery so the modal reflects the locations that actually remain.
        await get().fetchSkills(true);
        set((state) => {
          state.error = formatError(err, "Nie udało się usunąć skillu");
          state.resourceErrors[skillId] = state.error;
        });
      } finally {
        set((state) => {
          state.deletingSkillId = null;
        });
      }
    },

    setRepositoryOverride: async (
      skillId: string,
      repository: string | null,
    ) => {
      try {
        await api.setRepositoryOverride(skillId, repository);
        await get().fetchSkills(true);
        if (
          get().skills.some((skill) => skill.id === skillId && skill.remoteUrl)
        ) {
          await get().checkGitHubUpdate(skillId);
        }
      } catch (err: unknown) {
        set((state) => {
          state.error = formatError(
            err,
            "Nie udało się zapisać repozytorium GitHub do śledzenia",
          );
          state.resourceErrors[skillId] = state.error;
        });
      }
    },

    batchUpdateAll: async () => {
      const outdated = get().skills.filter((s) => s.updateAvailable);
      if (outdated.length === 0) return;

      set((state) => {
        state.batchUpdating = true;
        state.batchProgress = {
          total: outdated.length,
          completed: 0,
        };
        for (const skill of outdated) {
          const idx = state.skills.findIndex((s) => s.id === skill.id);
          if (idx !== -1) state.skills[idx].status = "updating";
        }
      });

      for (let i = 0; i < outdated.length; i++) {
        const skill = outdated[i];
        set((state) => {
          state.batchProgress.currentItem = skill.name;
          delete state.resourceErrors[skill.id];
        });

        try {
          const updated = await api.updateSingleSkill(skill.id);
          set((state) => {
            const idx = state.skills.findIndex((s) => s.id === skill.id);
            if (idx !== -1) state.skills[idx] = updated;
            state.batchProgress.completed = i + 1;
          });
        } catch (e) {
          console.error(`Failed to update ${skill.name}:`, e);
          const message = formatError(
            e,
            `Nie udało się zaktualizować ${skill.name}`,
          );
          set((state) => {
            const idx = state.skills.findIndex((s) => s.id === skill.id);
            if (idx !== -1) {
              state.skills[idx].status = "error";
              // Keep the update actionable after the user resolves the
              // underlying cause (for example, a dirty Git worktree).
              state.skills[idx].updateAvailable = true;
            }
            if (state.selectedSkill?.id === skill.id) {
              state.selectedSkill.status = "error";
              state.selectedSkill.updateAvailable = true;
            }
            state.error = message;
            state.resourceErrors[skill.id] = message;
            if (
              message.includes(
                "Katalog roboczy zawiera niezacommitowane zmiany",
              ) &&
              state.pendingDirtyUpdate === null
            ) {
              state.pendingDirtyUpdate = {
                skillId: skill.id,
                skillName: skill.name,
                reason: message,
              };
            }
            state.batchProgress.completed = i + 1;
          });
        }
      }

      set((state) => {
        state.batchUpdating = false;
        state.batchProgress = { total: 0, completed: 0 };
      });
    },

    rollbackSkill: async (skillId: string, snapshotId?: string) => {
      try {
        await api.rollbackSkill(skillId, snapshotId);
        await get().fetchSkills(true);
      } catch (err: unknown) {
        set((state) => {
          state.error = formatError(err, "Rollback failed");
          state.resourceErrors[skillId] = state.error;
        });
      }
    },

    checkAppUpdate: async () => {
      set((state) => {
        state.isCheckingAppUpdate = true;
        state.appUpdateError = null;
        state.appUpdateProgress.phase = "checking";
      });
      try {
        const appUpdate = await api.checkAppUpdate();
        set((state) => {
          state.appUpdate = appUpdate;
          state.isCheckingAppUpdate = false;
          state.appUpdateProgress.phase = "idle";
        });
      } catch (err: unknown) {
        set((state) => {
          state.isCheckingAppUpdate = false;
          state.appUpdateError = formatError(
            err,
            "Nie udało się sprawdzić aktualizacji SkillSync.",
          );
          state.appUpdateProgress.phase = "error";
        });
      }
    },

    installAppUpdate: async () => {
      if (!get().appUpdate?.updateAvailable) return;

      set((state) => {
        state.appUpdateError = null;
        state.appUpdateProgress = {
          phase: "checking",
          downloadedBytes: 0,
          contentLength: null,
        };
      });

      try {
        await api.installAppUpdate((progress) => {
          set((state) => {
            state.appUpdateProgress = {
              ...progress,
              contentLength:
                progress.contentLength ?? state.appUpdateProgress.contentLength,
            };
          });
        });
      } catch (err: unknown) {
        set((state) => {
          state.appUpdateError = formatError(
            err,
            "SkillSync could not install the update.",
          );
          state.appUpdateProgress.phase = "error";
        });
      }
    },

    setTheme: (theme: "dark" | "light") => {
      set((state) => {
        state.theme = theme;
      });
      if (typeof document !== "undefined") {
        const root = document.documentElement;
        if (theme === "dark") {
          root.classList.add("dark");
        } else {
          root.classList.remove("dark");
        }
      }
    },

    toggleTheme: () => {
      const current = get().theme;
      const next = current === "dark" ? "light" : "dark";
      get().setTheme(next);
    },

    setLanguage: (lang: Language) => {
      set((state) => {
        state.language = lang;
      });
      if (typeof window !== "undefined") {
        localStorage.setItem("skillsync_lang", lang);
      }
    },

    loadConfig: async () => {
      try {
        const config = await api.getConfig();
        set((state) => {
          state.config = config;
        });
      } catch (e) {
        console.error("Failed to load config:", e);
      }
    },

    saveConfig: async (config: AppConfig) => {
      try {
        await api.saveConfig(config);
        set((state) => {
          state.config = config;
        });
      } catch (e) {
        console.error("Failed to save config:", e);
      }
    },
  })),
);
