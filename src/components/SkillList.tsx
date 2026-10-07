import React from "react";
import { motion } from "framer-motion";
import { useSkillStore } from "../store/useSkillStore";
import { useTranslation } from "../i18n/useTranslation";
import { SkillCard } from "./SkillCard";
import { SearchX, FolderPlus, RefreshCw } from "lucide-react";
import { hasUpdateSource } from "../lib/branch-tracking";

export const SkillList: React.FC = () => {
  const { t } = useTranslation();
  const {
    skills,
    isLoading,
    searchQuery,
    selectedScope,
    statusFilter,
    itemTypeFilter,
    branchFilter,
    fetchSkills,
    openSettings,
  } = useSkillStore();

  // Filter skills
  const filteredSkills = skills
    .filter((skill) => {
      // Scope filter
      if (
        selectedScope !== "all" &&
        skill.agentScope.toLowerCase() !== selectedScope.toLowerCase()
      ) {
        return false;
      }

      if (branchFilter === "missing" && hasUpdateSource(skill)) {
        return false;
      }

      // Status filter
      if (statusFilter === "updates" && !skill.updateAvailable) return false;
      if (statusFilter === "up_to_date" && skill.updateAvailable) return false;

      if (
        itemTypeFilter !== "all" &&
        (skill.itemType ?? "skill") !== itemTypeFilter
      ) {
        return false;
      }

      // Search query
      if (searchQuery.trim() !== "") {
        const q = searchQuery.toLowerCase();
        const matchName = (skill.displayName ?? skill.name)
          .toLowerCase()
          .includes(q);
        const matchDesc = skill.description.toLowerCase().includes(q);
        const matchAuthor = skill.author.toLowerCase().includes(q);
        return matchName || matchDesc || matchAuthor;
      }

      return true;
    })
    .sort((left, right) => {
      const order = { skill: 0, mcp: 1, plugin: 2 } as const;
      const itemComparison =
        order[left.itemType ?? "skill"] - order[right.itemType ?? "skill"];
      return itemComparison !== 0
        ? itemComparison
        : left.name.localeCompare(right.name, undefined, {
            sensitivity: "base",
          });
    });

  // Skeleton loading state
  if (isLoading) {
    return (
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {[1, 2, 3, 4, 5, 6].map((n) => (
          <div
            key={n}
            className="h-44 rounded-xl border border-border bg-card/40 p-5 animate-pulse flex flex-col justify-between"
          >
            <div>
              <div className="flex justify-between items-center mb-3">
                <div className="h-4 w-16 bg-muted rounded"></div>
                <div className="h-4 w-20 bg-muted rounded-full"></div>
              </div>
              <div className="h-5 w-40 bg-muted rounded mb-2"></div>
              <div className="h-3 w-full bg-muted/60 rounded mb-1"></div>
              <div className="h-3 w-3/4 bg-muted/60 rounded"></div>
            </div>
            <div className="flex justify-between items-center pt-3 border-t border-border/40">
              <div className="h-3 w-16 bg-muted rounded"></div>
              <div className="h-6 w-20 bg-muted rounded"></div>
            </div>
          </div>
        ))}
      </div>
    );
  }

  // Empty state
  if (filteredSkills.length === 0) {
    return (
      <div className="text-center py-16 px-4 border border-dashed border-border rounded-2xl bg-card/30">
        <div className="w-12 h-12 rounded-xl bg-muted/50 border border-border mx-auto flex items-center justify-center text-muted-foreground mb-4">
          <SearchX className="w-6 h-6" />
        </div>
        <h3 className="text-base font-semibold text-foreground">
          {t("noSkillsFound")}
        </h3>
        <p className="mt-1 text-xs text-muted-foreground max-w-sm mx-auto">
          {searchQuery
            ? `${t("noSkillsMatchQuery")} "${searchQuery}".`
            : t("noSkillsInPath")}
        </p>
        <div className="mt-6 flex items-center justify-center gap-3">
          <button
            onClick={() => fetchSkills(true)}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border border-border bg-card hover:bg-muted text-foreground transition-all"
          >
            <RefreshCw className="w-3.5 h-3.5" />
            {t("rescanNow")}
          </button>
          <button
            onClick={openSettings}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition-all"
          >
            <FolderPlus className="w-3.5 h-3.5" />
            {t("configurePaths")}
          </button>
        </div>
      </div>
    );
  }

  return (
    <motion.div
      initial="hidden"
      animate="visible"
      variants={{
        hidden: { opacity: 0 },
        visible: {
          opacity: 1,
          transition: { staggerChildren: 0.04 },
        },
      }}
      className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4"
    >
      {filteredSkills.map((skill) => (
        <SkillCard key={skill.id} skill={skill} />
      ))}
    </motion.div>
  );
};
