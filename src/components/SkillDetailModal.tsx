import React, { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useSkillStore } from "../store/useSkillStore";
import { useTranslation } from "../i18n";
import { api } from "../lib/ipc";
import { formatSkillVersion } from "../lib/version-display";
import { BackupSnapshot } from "../types/skillsync";
import {
  X,
  ArrowUpCircle,
  RotateCcw,
  ExternalLink,
  GitBranch,
  Folder,
  Shield,
  Layers,
  Copy,
  Check,
  Loader2,
  RefreshCw,
  Download,
  CheckCircle2,
  Trash2,
  AlertTriangle,
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

export const SkillDetailModal: React.FC = () => {
  const { t, language } = useTranslation();
  const {
    selectedSkill,
    isDetailOpen,
    closeDetail,
    updateSkill,
    rollbackSkill,
    checkGitHubUpdate,
    checkoutCustomVersion,
    setBranchOverride,
    setRepositoryOverride,
    removeSkill,
    deletingSkillId,
    error,
  } = useSkillStore();

  const [copied, setCopied] = useState(false);
  const [backups, setBackups] = useState<BackupSnapshot[]>([]);
  const [selectedSnapshot, setSelectedSnapshot] = useState<string>("");
  const [isRollingBack, setIsRollingBack] = useState(false);
  const [isCheckingGitHub, setIsCheckingGitHub] = useState(false);
  const [customTagInput, setCustomTagInput] = useState("");
  const [isInstallingCustom, setIsInstallingCustom] = useState(false);
  const [branchInput, setBranchInput] = useState("");
  const [repositoryInput, setRepositoryInput] = useState("");

  useEffect(() => {
    if (selectedSkill) {
      api.getBackups(selectedSkill.id).then((b) => {
        setBackups(b);
        if (b.length > 0) setSelectedSnapshot(b[0].snapshotId);
      });
      // The custom-version field belongs to the selected skill and must reset on selection.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setCustomTagInput("");
      setBranchInput(
        selectedSkill.branchOverride ?? selectedSkill.detectedBranch ?? "",
      );
      setRepositoryInput(selectedSkill.remoteUrl ?? "");
    }
  }, [selectedSkill]);

  if (!isDetailOpen || !selectedSkill) return null;

  const copyPath = () => {
    navigator.clipboard.writeText(selectedSkill.path);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleCheckGitHub = async () => {
    setIsCheckingGitHub(true);
    await checkGitHubUpdate(selectedSkill.id);
    setIsCheckingGitHub(false);
  };

  const handleInstallCustomVersion = async () => {
    if (!customTagInput.trim()) return;
    setIsInstallingCustom(true);
    await checkoutCustomVersion(selectedSkill.id, customTagInput.trim());
    setIsInstallingCustom(false);
    setCustomTagInput("");
  };

  const handleSaveBranch = async () => {
    await setBranchOverride(selectedSkill.id, branchInput.trim() || null);
  };

  const handleSaveRepository = async () => {
    await setRepositoryOverride(
      selectedSkill.id,
      repositoryInput.trim() || null,
    );
  };

  const handleRollback = async () => {
    if (
      !confirm(
        "Are you sure you want to rollback to the selected backup point?",
      )
    )
      return;
    setIsRollingBack(true);
    await rollbackSkill(selectedSkill.id, selectedSnapshot);
    setIsRollingBack(false);
    closeDetail();
  };

  const handleOpenFolder = () => {
    api.openInEditor(selectedSkill.path);
  };

  const handleOpenRepo = () => {
    if (selectedSkill.remoteUrl) {
      api.openUrl(selectedSkill.remoteUrl);
    }
  };

  const installedLocations =
    selectedSkill.installedLocations &&
    selectedSkill.installedLocations.length > 0
      ? selectedSkill.installedLocations
      : [selectedSkill.path];
  const isDeleting = deletingSkillId === selectedSkill.id;

  const handleDeleteLocations = async (locations: string[]) => {
    const allLocations = locations.length === installedLocations.length;
    const targetLabel = allLocations
      ? "we wszystkich lokalizacjach"
      : "w wybranej lokalizacji";
    if (
      !confirm(
        `Usunąć „${selectedSkill.name}” ${targetLabel}? Przed usunięciem katalogów SkillSync utworzy migawkę bezpieczeństwa.`,
      )
    ) {
      return;
    }
    await removeSkill(selectedSkill.id, locations);
  };

  return (
    <AnimatePresence>
      <div className="fixed inset-0 z-50 flex items-center justify-center p-4 sm:p-6 md:p-10">
        {/* Backdrop */}
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={closeDetail}
          className="fixed inset-0 bg-background/80 backdrop-blur-sm"
        />

        {/* Modal Window */}
        <motion.div
          initial={{ opacity: 0, scale: 0.95, y: 10 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.95, y: 10 }}
          transition={{ type: "spring", stiffness: 350, damping: 25 }}
          className="relative w-full max-w-3xl max-h-[85vh] overflow-y-auto rounded-2xl border border-border bg-card shadow-2xl p-6 sm:p-8 flex flex-col justify-between"
        >
          <div>
            {error && (
              <div
                role="alert"
                className="mb-4 flex items-start gap-2 rounded-lg border border-rose-500/30 bg-rose-500/10 p-3 text-xs text-rose-300"
              >
                <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
                <span>{error}</span>
              </div>
            )}
            {/* Header */}
            <div className="flex items-start justify-between gap-4 pb-4 border-b border-border">
              <div>
                <div className="flex items-center gap-2 mb-1">
                  <span className="px-2 py-0.5 rounded text-[10px] font-semibold uppercase tracking-wider bg-primary/15 text-primary border border-primary/25">
                    {selectedSkill.agentScope}
                  </span>
                  <span className="px-2 py-0.5 rounded text-[10px] font-semibold uppercase tracking-wider bg-sky-500/10 text-sky-400 border border-sky-500/30">
                    {selectedSkill.itemType ?? "skill"}
                  </span>
                  <span className="text-xs font-mono text-muted-foreground">
                    {formatSkillVersion(selectedSkill.currentVersion, language)}
                  </span>
                  {selectedSkill.updateAvailable &&
                    selectedSkill.latestVersion &&
                    selectedSkill.latestVersion !==
                      selectedSkill.currentVersion && (
                      <span className="px-2 py-0.5 rounded-full text-[10px] font-medium bg-amber-500/15 text-amber-400 border border-amber-500/30">
                        {t.updateAvailable}:{" "}
                        {formatSkillVersion(
                          selectedSkill.latestVersion,
                          language,
                        )}
                      </span>
                    )}
                </div>
                <h2 className="text-xl font-bold text-foreground tracking-tight">
                  {selectedSkill.displayName ?? selectedSkill.name}
                </h2>
                <p className="text-xs text-muted-foreground mt-0.5">
                  Author:{" "}
                  <span className="text-foreground font-medium">
                    {selectedSkill.author}
                  </span>
                </p>
              </div>

              <div className="flex items-center gap-2">
                {selectedSkill.remoteUrl && (
                  <button
                    onClick={handleOpenRepo}
                    className="px-3 py-1.5 text-xs font-semibold rounded-lg bg-primary/10 hover:bg-primary/20 text-primary border border-primary/30 transition-all flex items-center gap-1.5 shadow-sm"
                    title={selectedSkill.remoteUrl}
                  >
                    <GithubIcon className="w-4 h-4" />
                    <span className="hidden sm:inline">{t.openRepo}</span>
                    <ExternalLink className="w-3 h-3 text-primary/70" />
                  </button>
                )}

                <button
                  onClick={closeDetail}
                  className="w-8 h-8 rounded-lg flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-muted transition-all"
                >
                  <X className="w-5 h-5" />
                </button>
              </div>
            </div>

            {/* Live Update Progress Banner */}
            {selectedSkill.status === "updating" && (
              <div className="mt-4 p-4 rounded-xl border border-primary/40 bg-primary/10 animate-pulse flex items-center gap-3">
                <Loader2 className="w-5 h-5 text-primary animate-spin shrink-0" />
                <div className="flex-1">
                  <h4 className="text-xs font-semibold text-primary">
                    {t.updatingLocations}
                  </h4>
                  <p className="text-[11px] text-muted-foreground mt-0.5">
                    Tworzenie migawki bezpieczeństwa, preflight, aktualizacja i
                    weryfikacja integralności we wszystkich lokalizacjach (
                    {selectedSkill.installedLocations?.length || 1}).
                  </p>
                </div>
              </div>
            )}

            {/* Description */}
            <p className="mt-4 text-sm text-foreground/90 leading-relaxed">
              {selectedSkill.description}
            </p>

            {/* Path box */}
            <div className="mt-4 p-2.5 rounded-lg bg-muted/40 border border-border flex items-center justify-between gap-2 text-xs font-mono">
              <span className="truncate text-muted-foreground select-all">
                {selectedSkill.path}
              </span>
              <button
                onClick={copyPath}
                className="flex items-center gap-1 text-[11px] font-sans text-muted-foreground hover:text-foreground px-2 py-1 rounded hover:bg-muted transition-all shrink-0"
              >
                {copied ? (
                  <Check className="w-3.5 h-3.5 text-emerald-400" />
                ) : (
                  <Copy className="w-3.5 h-3.5" />
                )}
                {copied ? "Copied" : "Copy"}
              </button>
            </div>

            {/* Grid details */}
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 mt-5">
              {/* Git details */}
              <div className="p-4 rounded-xl border border-border bg-card/50">
                <div className="flex items-center justify-between text-xs font-semibold text-foreground mb-2">
                  <span className="flex items-center gap-2">
                    <GitBranch className="w-4 h-4 text-primary" />
                    {t.versionControl}
                  </span>
                  {selectedSkill.remoteUrl && (
                    <button
                      onClick={handleCheckGitHub}
                      disabled={isCheckingGitHub}
                      className="px-2 py-0.5 text-[10px] font-medium rounded border border-border bg-card hover:bg-muted text-foreground flex items-center gap-1 transition-all disabled:opacity-50"
                    >
                      <RefreshCw
                        className={`w-2.5 h-2.5 ${isCheckingGitHub ? "animate-spin text-primary" : ""}`}
                      />
                      {isCheckingGitHub ? t.checking : t.checkGitHub}
                    </button>
                  )}
                </div>
                <div className="space-y-1.5 text-xs text-muted-foreground">
                  <div className="flex justify-between items-center">
                    <span>Remote:</span>
                    {selectedSkill.remoteUrl ? (
                      <button
                        onClick={handleOpenRepo}
                        className="text-primary hover:underline truncate max-w-[200px] inline-flex items-center gap-1 cursor-pointer text-left"
                      >
                        <span className="truncate">
                          {selectedSkill.remoteUrl.replace("https://", "")}
                        </span>
                        <ExternalLink className="w-3 h-3 shrink-0" />
                      </button>
                    ) : (
                      <span className="text-foreground">Local only</span>
                    )}
                  </div>
                  <div className="flex justify-between">
                    <span>Active Ref:</span>
                    <span className="text-foreground font-mono">
                      {selectedSkill.branchOrTag || "HEAD"}
                    </span>
                  </div>
                  <div className="mt-3 border-t border-border/60 pt-3">
                    <label className="mb-1 block text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
                      Repozytorium GitHub do śledzenia
                    </label>
                    <div className="flex gap-1.5">
                      <input
                        value={repositoryInput}
                        onChange={(event) =>
                          setRepositoryInput(event.target.value)
                        }
                        placeholder="https://github.com/PrefectHQ/fastmcp"
                        className="min-w-0 flex-1 rounded border border-border bg-background px-2 py-1.5 font-mono text-[11px] text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
                      />
                      <button
                        onClick={handleSaveRepository}
                        className="rounded border border-primary/30 bg-primary/10 px-2 py-1 text-[10px] font-semibold text-primary transition-colors hover:bg-primary/20"
                      >
                        Zapisz
                      </button>
                    </div>
                    <p className="mt-1 text-[10px] leading-relaxed text-muted-foreground">
                      Dla lokalnych skillów bez Git. Wpisz główny adres
                      repozytorium; pusta wartość przywraca auto-wykrywanie.
                    </p>
                  </div>
                  <div className="mt-3 border-t border-border/60 pt-3">
                    <label className="mb-1 block text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
                      Śledzona gałąź{" "}
                      {selectedSkill.branchOverride
                        ? "(ręcznie ustawiona)"
                        : "(wykryta automatycznie)"}
                    </label>
                    <div className="flex gap-1.5">
                      <input
                        value={branchInput}
                        onChange={(event) => setBranchInput(event.target.value)}
                        placeholder="np. main, develop, next"
                        className="min-w-0 flex-1 rounded border border-border bg-background px-2 py-1.5 font-mono text-[11px] text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
                      />
                      <button
                        onClick={handleSaveBranch}
                        className="rounded border border-primary/30 bg-primary/10 px-2 py-1 text-[10px] font-semibold text-primary transition-colors hover:bg-primary/20"
                      >
                        Zapisz
                      </button>
                    </div>
                    <p className="mt-1 text-[10px] leading-relaxed text-muted-foreground">
                      Pusta wartość przywraca auto-wykrywanie. Ręczna gałąź ma
                      priorytet i jej rozbieżność oznacza pakiet do
                      aktualizacji.
                    </p>
                  </div>
                </div>
              </div>

              {/* Permissions & Dependencies */}
              <div className="p-4 rounded-xl border border-border bg-card/50">
                <div className="flex items-center gap-2 text-xs font-semibold text-foreground mb-2">
                  <Shield className="w-4 h-4 text-amber-400" />
                  <span>{t.permissionsAndDeps}</span>
                </div>
                <div className="flex flex-wrap gap-1 mt-1">
                  {selectedSkill.permissions.length > 0 ? (
                    selectedSkill.permissions.map((p) => (
                      <span
                        key={p}
                        className="px-1.5 py-0.5 rounded text-[10px] bg-amber-500/10 text-amber-400 border border-amber-500/20 font-mono"
                      >
                        {p}
                      </span>
                    ))
                  ) : (
                    <span className="text-xs text-muted-foreground">
                      Standard permissions
                    </span>
                  )}
                </div>
              </div>

              {/* Compatibility & Environment Section */}
              <div className="p-4 rounded-xl border border-border bg-card/50 sm:col-span-2">
                <div className="flex items-center gap-2 text-xs font-semibold text-foreground mb-2">
                  <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                  <span>
                    {t.compatibility} &amp; {t.targetEnvironment}
                  </span>
                </div>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs text-muted-foreground">
                  <div className="p-2.5 rounded-lg bg-background/50 border border-border/60">
                    <p className="font-semibold text-foreground text-[11px] mb-1">
                      {t.targetEnvironment}:
                    </p>
                    <p className="text-foreground/90 font-medium">
                      {selectedSkill.compatibility ||
                        `${selectedSkill.agentScope.toUpperCase()} Environment`}
                    </p>
                  </div>
                  <div className="p-2.5 rounded-lg bg-background/50 border border-border/60">
                    <p className="font-semibold text-foreground text-[11px] mb-1">
                      {t.updateCompatibility}:
                    </p>
                    <p
                      className={
                        selectedSkill.updateAvailable
                          ? "text-amber-400 font-medium"
                          : "text-emerald-400 font-medium"
                      }
                    >
                      {selectedSkill.updateCompatibility ||
                        (selectedSkill.updateAvailable
                          ? "SemVer Minor: Wstecznie kompatybilna aktualizacja (brak zmian łamiących API)"
                          : "Wersja bieżąca jest stabilna i w pełni kompatybilna")}
                    </p>
                  </div>
                </div>
              </div>

              {/* Installed Locations Multi-Sync Section */}
              {installedLocations.length > 0 && (
                <div className="p-4 rounded-xl border border-border bg-card/50 sm:col-span-2">
                  <div className="flex items-center justify-between text-xs font-semibold text-foreground mb-2">
                    <div className="flex items-center gap-2">
                      <Folder className="w-4 h-4 text-primary" />
                      <span>
                        {t.installedLocations} ({installedLocations.length})
                      </span>
                    </div>
                    <span className="text-[10px] text-muted-foreground font-mono">
                      Symlink &amp; Multi-Directory Sync
                    </span>
                  </div>
                  <div className="space-y-1.5 max-h-36 overflow-y-auto pr-1">
                    {installedLocations.map((loc, idx) => (
                      <div
                        key={idx}
                        className="flex items-center justify-between p-2 rounded-lg bg-background/60 border border-border/60 text-xs font-mono"
                      >
                        <span
                          className="text-foreground/90 truncate mr-2 text-[11px]"
                          title={loc}
                        >
                          {loc}
                        </span>
                        <div className="flex items-center gap-1.5 shrink-0 font-sans">
                          <span className="px-1.5 py-0.5 rounded text-[10px] bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 flex items-center gap-1">
                            <Check className="w-2.5 h-2.5" />
                            Aktywny
                          </span>
                          <button
                            onClick={() => handleDeleteLocations([loc])}
                            disabled={isDeleting}
                            className="rounded border border-destructive/40 px-1.5 py-0.5 text-[10px] font-semibold text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
                            title="Usuń tylko tę lokalizację"
                          >
                            Usuń
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>

            {/* Manual Tag / Version Checkout Section */}
            {selectedSkill.isGitRepo && (
              <div className="mt-5 p-4 rounded-xl border border-border bg-card/50">
                <div className="flex items-center gap-2 text-xs font-semibold text-foreground mb-1">
                  <Download className="w-4 h-4 text-primary" />
                  <span>{t.manualVersionTitle}</span>
                </div>
                <p className="text-[11px] text-muted-foreground mb-3">
                  {t.manualVersionDesc}
                </p>
                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    value={customTagInput}
                    onChange={(e) => setCustomTagInput(e.target.value)}
                    placeholder={t.targetTagPlaceholder}
                    className="flex-1 h-8 px-3 text-xs rounded-lg border border-border bg-background text-foreground font-mono placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary"
                  />
                  <button
                    onClick={handleInstallCustomVersion}
                    disabled={!customTagInput.trim() || isInstallingCustom}
                    className="h-8 px-3 text-xs font-medium rounded-lg bg-primary hover:bg-primary/90 text-primary-foreground shadow-sm flex items-center gap-1 transition-all disabled:opacity-40"
                  >
                    {isInstallingCustom ? (
                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    ) : (
                      <GitBranch className="w-3.5 h-3.5" />
                    )}
                    {t.installTag}
                  </button>
                </div>
              </div>
            )}

            {/* Changelog preview */}
            {selectedSkill.changelog && (
              <div className="mt-5 p-4 rounded-xl border border-border bg-card/50">
                <div className="flex items-center gap-2 text-xs font-semibold text-foreground mb-2">
                  <Layers className="w-4 h-4 text-primary" />
                  <span>{t.releaseNotes}</span>
                </div>
                <div className="p-3 rounded-lg bg-background border border-border font-mono text-xs text-foreground/80 whitespace-pre-wrap leading-relaxed max-h-40 overflow-y-auto">
                  {selectedSkill.changelog}
                </div>
              </div>
            )}

            {/* Rollback Section */}
            {backups.length > 0 && (
              <div className="mt-5 p-4 rounded-xl border border-border bg-card/50 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3">
                <div>
                  <h4 className="text-xs font-semibold text-foreground flex items-center gap-1.5">
                    <RotateCcw className="w-3.5 h-3.5 text-muted-foreground" />
                    {t.pointInTimeRollback}
                  </h4>
                  <p className="text-[11px] text-muted-foreground mt-0.5">
                    {t.rollbackDescription}
                  </p>
                </div>

                <div className="flex items-center gap-2 w-full sm:w-auto">
                  <select
                    value={selectedSnapshot}
                    onChange={(e) => setSelectedSnapshot(e.target.value)}
                    className="h-8 px-2 text-xs rounded-lg border border-border bg-card text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
                  >
                    {backups.map((b) => {
                      const d = new Date(b.createdAt);
                      const dateStr = d.toLocaleDateString();
                      const timeStr = d.toLocaleTimeString([], {
                        hour: "2-digit",
                        minute: "2-digit",
                        second: "2-digit",
                      });
                      return (
                        <option key={b.snapshotId} value={b.snapshotId}>
                          v{b.originalVersion} ({dateStr} {timeStr})
                        </option>
                      );
                    })}
                  </select>

                  <button
                    onClick={handleRollback}
                    disabled={isRollingBack}
                    className="h-8 px-3 text-xs font-medium rounded-lg border border-destructive/40 text-destructive hover:bg-destructive/10 transition-all flex items-center gap-1 shrink-0"
                  >
                    {isRollingBack ? (
                      <Loader2 className="w-3 h-3 animate-spin" />
                    ) : (
                      <RotateCcw className="w-3 h-3" />
                    )}
                    {t.restore}
                  </button>
                </div>
              </div>
            )}
          </div>

          {/* Footer action bar */}
          <div className="mt-6 pt-4 border-t border-border flex items-center justify-between gap-3">
            <button
              onClick={handleOpenFolder}
              className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border border-border bg-card hover:bg-muted text-foreground transition-all"
            >
              <Folder className="w-3.5 h-3.5 text-muted-foreground" />
              {t.openFolder}
            </button>

            <div className="flex items-center gap-2">
              <button
                onClick={() => handleDeleteLocations(installedLocations)}
                disabled={isDeleting}
                className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border border-destructive/40 text-destructive hover:bg-destructive/10 transition-all disabled:opacity-50"
                title="Usuń skill ze wszystkich wskazanych lokalizacji"
              >
                {isDeleting ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <Trash2 className="w-3.5 h-3.5" />
                )}
                Usuń {installedLocations.length > 1 ? "wszędzie" : "skill"}
              </button>
              <button
                onClick={closeDetail}
                className="px-3.5 py-1.5 text-xs font-medium rounded-lg border border-border text-foreground hover:bg-muted transition-all"
              >
                {t.close}
              </button>

              {selectedSkill.updateAvailable && (
                <button
                  onClick={() => updateSkill(selectedSkill.id)}
                  disabled={selectedSkill.status === "updating"}
                  className="flex items-center gap-1.5 px-4 py-1.5 text-xs font-semibold rounded-lg bg-primary hover:bg-primary/90 text-primary-foreground shadow-sm shadow-primary/25 transition-all disabled:opacity-50"
                >
                  {selectedSkill.status === "updating" ? (
                    <Loader2 className="w-4 h-4 animate-spin" />
                  ) : (
                    <ArrowUpCircle className="w-4 h-4" />
                  )}
                  {selectedSkill.status === "updating"
                    ? t.updating
                    : `${t.updateTo} v${selectedSkill.latestVersion}`}
                </button>
              )}
            </div>
          </div>
        </motion.div>
      </div>
    </AnimatePresence>
  );
};
