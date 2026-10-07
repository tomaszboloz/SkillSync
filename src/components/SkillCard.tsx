import React, { useState } from "react";
import { motion } from "framer-motion";
import { SkillMetadata } from "../types/skillsync";
import { useSkillStore } from "../store/useSkillStore";
import { useTranslation } from "../i18n";
import { getScopeBadgeColor } from "../lib/utils";
import { formatSkillVersion } from "../lib/version-display";
import { api } from "../lib/ipc";
import { resourceError } from "../lib/resource-error";
import {
  ArrowRight,
  GitBranch,
  Folder,
  ArrowUpCircle,
  CheckCircle2,
  AlertTriangle,
  FileCode,
  Loader2,
  RefreshCw,
} from "lucide-react";

const GithubIcon = ({ className = "w-3.5 h-3.5" }: { className?: string }) => (
  <svg className={className} viewBox="0 0 24 24" fill="currentColor">
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.53 1.032 1.53 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z"
    />
  </svg>
);

interface SkillCardProps {
  skill: SkillMetadata;
}

export const SkillCard: React.FC<SkillCardProps> = ({ skill }) => {
  const { t, language } = useTranslation();
  const { openDetail, updateSkill, checkGitHubUpdate, resourceErrors } =
    useSkillStore();
  const [isCheckingGitHub, setIsCheckingGitHub] = useState(false);

  const isUpdating = skill.status === "updating";
  const itemType = skill.itemType ?? "skill";

  const handleCheckGitHub = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsCheckingGitHub(true);
    await checkGitHubUpdate(skill.id);
    setIsCheckingGitHub(false);
  };

  const renderStatusBadge = () => {
    if (isUpdating) {
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-primary/15 text-primary border border-primary/30">
          <Loader2 className="w-3 h-3 animate-spin" />
          {t.updating}
        </span>
      );
    }
    if (skill.status === "checking") {
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-sky-500/15 text-sky-400 border border-sky-500/30">
          <Loader2 className="w-3 h-3 animate-spin" />
          {t.checking}
        </span>
      );
    }
    if (skill.updateAvailable) {
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-amber-500/15 text-amber-400 border border-amber-500/30">
          <ArrowUpCircle className="w-3 h-3" />
          {t.updateAvailable}
        </span>
      );
    }
    if (skill.status === "modified_locally") {
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-purple-500/15 text-purple-400 border border-purple-500/30">
          <AlertTriangle className="w-3 h-3" />
          {t.modifiedLocally}
        </span>
      );
    }
    if (skill.status === "error" || resourceError(skill, resourceErrors)) {
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-rose-500/15 text-rose-400 border border-rose-500/30">
          <AlertTriangle className="w-3 h-3" />
          {t.error}
        </span>
      );
    }
    return (
      <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-emerald-500/15 text-emerald-400 border border-emerald-500/30">
        <CheckCircle2 className="w-3 h-3" />
        {t.upToDate}
      </span>
    );
  };

  return (
    <motion.div
      layout
      whileHover={{ y: -2, transition: { duration: 0.15 } }}
      onClick={() => openDetail(skill)}
      className={`group relative rounded-xl border p-5 cursor-pointer shadow-sm hover:shadow-md transition-all flex flex-col justify-between overflow-hidden ${
        isUpdating
          ? "border-primary ring-2 ring-primary/30 bg-primary/5"
          : "border-border bg-card/70 hover:bg-card"
      }`}
    >
      <div>
        {/* Top meta row */}
        <div className="flex items-start justify-between gap-3 mb-3">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider rounded border border-sky-500/30 bg-sky-500/10 text-sky-400">
              {itemType}
            </span>
            <span
              className={`px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider rounded border ${getScopeBadgeColor(
                skill.agentScope,
              )}`}
            >
              {skill.agentScope}
            </span>
            {skill.isGitRepo && (
              <span className="inline-flex items-center gap-1 px-1.5 py-0.5 text-[10px] text-muted-foreground bg-muted/60 rounded border border-border">
                <GitBranch className="w-2.5 h-2.5" />
                git
              </span>
            )}
            {skill.installedLocations &&
              skill.installedLocations.length > 1 && (
                <span
                  className="inline-flex items-center gap-1 px-1.5 py-0.5 text-[10px] text-muted-foreground bg-muted/60 rounded border border-border"
                  title={`${t.installedLocations}: \n${skill.installedLocations.join("\n")}`}
                >
                  📍 {skill.installedLocations.length} lok.
                </span>
              )}
          </div>
          <div>{renderStatusBadge()}</div>
        </div>

        {/* Title */}
        <h3 className="text-base font-semibold text-foreground tracking-tight group-hover:text-primary transition-colors flex items-center gap-1.5">
          <FileCode className="w-4 h-4 text-primary/70" />
          {skill.displayName ?? skill.name}
        </h3>

        {/* Description */}
        <p className="mt-1.5 text-xs text-muted-foreground line-clamp-2 leading-relaxed">
          {skill.description}
        </p>

        {/* Compatibility and Requirements */}
        <div className="mt-3 flex flex-wrap items-center gap-1.5">
          {skill.compatibility && (
            <span className="inline-flex items-center px-2 py-0.5 rounded text-[10px] font-medium bg-muted/70 text-foreground/80 border border-border/70">
              {skill.compatibility}
            </span>
          )}
          {skill.updateAvailable && skill.updateCompatibility && (
            <span
              className={`inline-flex items-center px-2 py-0.5 rounded text-[10px] font-medium border ${
                skill.updateCompatibility.includes("Major")
                  ? "bg-amber-500/10 text-amber-400 border-amber-500/30"
                  : "bg-emerald-500/10 text-emerald-400 border-emerald-500/30"
              }`}
              title={skill.updateCompatibility}
            >
              {skill.updateCompatibility.includes("Major")
                ? "⚠️ SemVer Major"
                : "✓ SemVer Minor"}
            </span>
          )}
        </div>
      </div>

      {/* Bottom info & actions */}
      <div className="mt-5 pt-3 border-t border-border/60 flex flex-wrap items-center justify-between gap-2 text-xs">
        {/* Version Parity Pill */}
        <div className="flex items-center gap-1 font-mono text-[11px] shrink-0">
          <span className="text-muted-foreground">
            {formatSkillVersion(skill.currentVersion, language)}
          </span>
          {skill.updateAvailable &&
            skill.latestVersion &&
            skill.latestVersion !== skill.currentVersion && (
              <>
                <ArrowRight className="w-3 h-3 text-amber-400" />
                <span className="text-amber-400 font-semibold">
                  {formatSkillVersion(skill.latestVersion, language)}
                </span>
              </>
            )}
        </div>

        {/* Actions */}
        <div
          className="flex items-center gap-1.5 flex-wrap ml-auto"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Open GitHub Repository Button */}
          {skill.remoteUrl && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                api.openUrl(skill.remoteUrl!);
              }}
              title={`${t.openRepo}: ${skill.remoteUrl}`}
              className="p-1.5 text-xs rounded-md border border-border bg-card hover:bg-muted text-muted-foreground hover:text-foreground transition-all flex items-center justify-center shrink-0 group/btn"
            >
              <GithubIcon className="w-3.5 h-3.5 group-hover/btn:text-primary transition-colors" />
            </button>
          )}

          {/* Check GitHub Button if git repo */}
          {skill.remoteUrl && (
            <button
              onClick={handleCheckGitHub}
              disabled={isCheckingGitHub}
              title={t.checkGitHub}
              className="p-1.5 text-xs rounded-md border border-border bg-card hover:bg-muted text-muted-foreground hover:text-foreground transition-all disabled:opacity-50 shrink-0"
            >
              <RefreshCw
                className={`w-3 h-3 ${isCheckingGitHub ? "animate-spin text-primary" : ""}`}
              />
            </button>
          )}

          {skill.updateAvailable && (
            <button
              onClick={() => updateSkill(skill.id)}
              disabled={isUpdating}
              className="px-2 py-1 text-[11px] font-semibold rounded-md bg-primary hover:bg-primary/90 text-primary-foreground shadow-sm transition-all disabled:opacity-50 flex items-center gap-1 shrink-0"
            >
              {isUpdating ? (
                <Loader2 className="w-3 h-3 animate-spin" />
              ) : (
                <ArrowUpCircle className="w-3 h-3" />
              )}
              {isUpdating ? t.updating : t.update}
            </button>
          )}

          <button
            onClick={() => openDetail(skill)}
            className="px-2 py-1 text-[11px] font-medium rounded-md border border-border bg-card hover:bg-muted text-foreground transition-all flex items-center gap-1 shrink-0"
            title={t.inspect}
          >
            <Folder className="w-3 h-3 text-muted-foreground" />
            {t.inspect}
          </button>
        </div>
      </div>
    </motion.div>
  );
};
