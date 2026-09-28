use crate::errors::SkillSyncError;
use crate::models::skill::BackupSnapshot;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub struct BackupService;

impl BackupService {
    pub fn get_backup_dir() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let dir = home.join(".skillsync").join("backups");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    pub fn create_snapshot(
        skill_path: &Path,
        skill_id: &str,
        current_version: &str,
    ) -> Result<BackupSnapshot, SkillSyncError> {
        let backup_dir = Self::get_backup_dir();
        let timestamp = chrono::Utc::now();
        // One transaction may snapshot several installations of the same
        // resource in the same second. Include nanoseconds so snapshots never
        // overwrite each other before rollback has a chance to use them.
        let filename = format!(
            "{}_{}_{}.tar.gz",
            skill_id,
            timestamp.format("%Y%m%d_%H%M%S"),
            timestamp.timestamp_nanos_opt().unwrap_or_default()
        );
        let target_file = backup_dir.join(&filename);

        let file = File::create(&target_file)?;
        let enc = GzEncoder::new(file, Compression::default());
        let mut tar = tar::Builder::new(enc);

        // A managed package may legitimately contain symlinks (including a
        // dangling link left by a package manager).  The tar crate follows
        // links by default, which both makes a snapshot escape its source
        // tree and turns a harmless dangling link into ENOENT.  Preserve the
        // link itself instead, just like the platform tar implementations.
        tar.follow_symlinks(false);
        tar.append_dir_all(".", skill_path)?;
        tar.finish()?;

        // The archive filename already carries nanoseconds; the public ID must
        // use the same precision so two locations snapshotted in one second
        // cannot accidentally select each other's rollback entry.
        let snap_id = format!(
            "snap-{}",
            timestamp.timestamp_nanos_opt().unwrap_or_default()
        );

        // Write metadata JSON sidecar file for exact version and timestamp persistence
        let meta_file = backup_dir.join(format!("{}.meta.json", filename));
        let meta_json = serde_json::json!({
            "snapshot_id": snap_id,
            "skill_id": skill_id,
            "created_at": timestamp.to_rfc3339(),
            "original_version": current_version,
            "filename": filename,
        });
        let _ = fs::write(&meta_file, meta_json.to_string());

        Ok(BackupSnapshot {
            snapshot_id: snap_id,
            skill_id: skill_id.to_string(),
            created_at: timestamp,
            backup_file_path: target_file,
            original_version: current_version.to_string(),
        })
    }

    pub fn restore_snapshot(skill_path: &Path, snapshot_file: &Path) -> Result<(), SkillSyncError> {
        if !snapshot_file.exists() {
            return Err(SkillSyncError::RollbackFailed(
                "Snapshot file not found".into(),
            ));
        }

        let target = Self::validated_restore_target(skill_path)?;
        Self::clear_directory_contents(&target)?;

        let file = File::open(snapshot_file)?;
        let dec = GzDecoder::new(file);
        let mut archive = tar::Archive::new(dec);

        // A rollback must be exact: an update may have added files (for example
        // Composer's vendor files), so extracting over the old tree is unsafe.
        archive
            .unpack(&target)
            .map_err(|e| SkillSyncError::RollbackFailed(e.to_string()))?;
        Ok(())
    }

    fn validated_restore_target(path: &Path) -> Result<PathBuf, SkillSyncError> {
        let target = fs::canonicalize(path).map_err(|error| {
            SkillSyncError::RollbackFailed(format!("Nie można ustalić katalogu rollbacku: {error}"))
        })?;
        let home = dirs::home_dir().and_then(|home| fs::canonicalize(home).ok());
        if !target.is_dir() || target.parent().is_none() || home.as_ref() == Some(&target) {
            return Err(SkillSyncError::RollbackFailed(format!(
                "Niebezpieczny katalog rollbacku: {}",
                target.display()
            )));
        }
        Ok(target)
    }

    fn clear_directory_contents(path: &Path) -> Result<(), SkillSyncError> {
        for entry in
            fs::read_dir(path).map_err(|error| SkillSyncError::RollbackFailed(error.to_string()))?
        {
            let entry = entry.map_err(|error| SkillSyncError::RollbackFailed(error.to_string()))?;
            let entry_path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| SkillSyncError::RollbackFailed(error.to_string()))?;
            if file_type.is_dir() {
                fs::remove_dir_all(&entry_path)
                    .map_err(|error| SkillSyncError::RollbackFailed(error.to_string()))?;
            } else {
                fs::remove_file(&entry_path)
                    .map_err(|error| SkillSyncError::RollbackFailed(error.to_string()))?;
            }
        }
        Ok(())
    }

    pub fn list_snapshots(skill_id: &str) -> Vec<BackupSnapshot> {
        let backup_dir = Self::get_backup_dir();
        let mut result = Vec::new();

        if let Ok(entries) = fs::read_dir(&backup_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with(skill_id) && name.ends_with(".tar.gz") {
                        let meta_path = backup_dir.join(format!("{}.meta.json", name));
                        let mut snap_created_at = None;
                        let mut original_version = "previous".to_string();
                        let mut snapshot_id = name.to_string();

                        if meta_path.exists() {
                            if let Ok(data) = fs::read_to_string(&meta_path) {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data) {
                                    if let Some(sid) =
                                        val.get("snapshot_id").and_then(|v| v.as_str())
                                    {
                                        snapshot_id = sid.to_string();
                                    }
                                    if let Some(ver) =
                                        val.get("original_version").and_then(|v| v.as_str())
                                    {
                                        original_version = ver.to_string();
                                    }
                                    if let Some(cat) =
                                        val.get("created_at").and_then(|v| v.as_str())
                                    {
                                        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(cat) {
                                            snap_created_at = Some(dt.with_timezone(&chrono::Utc));
                                        }
                                    }
                                }
                            }
                        }

                        // Fallback: parse timestamp from filename or file modification time
                        let created_at = snap_created_at.unwrap_or_else(|| {
                            // filename format: {skill_id}_YYYYmmdd_HHMMSS.tar.gz
                            let name_without_ext = name.trim_end_matches(".tar.gz");
                            if let Some(idx) = name_without_ext.rfind('_') {
                                let time_str = &name_without_ext[idx + 1..];
                                let rest = &name_without_ext[..idx];
                                if let Some(date_idx) = rest.rfind('_') {
                                    let date_str = &rest[date_idx + 1..];
                                    let combined = format!("{}_{}", date_str, time_str);
                                    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(
                                        &combined,
                                        "%Y%m%d_%H%M%S",
                                    ) {
                                        return chrono::DateTime::from_naive_utc_and_offset(
                                            naive,
                                            chrono::Utc,
                                        );
                                    }
                                }
                            }
                            if let Ok(meta) = entry.metadata() {
                                if let Ok(mod_time) = meta.modified() {
                                    let dt: chrono::DateTime<chrono::Utc> = mod_time.into();
                                    return dt;
                                }
                            }
                            chrono::Utc::now()
                        });

                        result.push(BackupSnapshot {
                            snapshot_id,
                            skill_id: skill_id.to_string(),
                            created_at,
                            backup_file_path: path,
                            original_version,
                        });
                    }
                }
            }
        }

        // Sort newest first
        result.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.created_at));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_removes_files_created_after_the_snapshot() {
        let root = std::env::temp_dir().join(format!(
            "skillsync-backup-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested/original.txt"), "before").unwrap();
        let snapshot = BackupService::create_snapshot(&root, "rollback-test", "1.0.0").unwrap();

        fs::write(root.join("new-file.txt"), "after").unwrap();
        fs::write(root.join("nested/original.txt"), "changed").unwrap();
        BackupService::restore_snapshot(&root, &snapshot.backup_file_path).unwrap();

        assert!(!root.join("new-file.txt").exists());
        assert_eq!(
            fs::read_to_string(root.join("nested/original.txt")).unwrap(),
            "before"
        );
        let _ = fs::remove_dir_all(root);
        let sidecar = snapshot.backup_file_path.with_file_name(format!(
            "{}.meta.json",
            snapshot
                .backup_file_path
                .file_name()
                .unwrap()
                .to_string_lossy()
        ));
        let _ = fs::remove_file(sidecar);
        let _ = fs::remove_file(snapshot.backup_file_path);
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_preserves_a_dangling_symlink_without_following_it() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "skillsync-backup-link-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root).unwrap();
        symlink("missing-target", root.join("stale-link")).unwrap();

        let snapshot = BackupService::create_snapshot(&root, "link-test", "1.0.0")
            .expect("a dangling symlink is valid backup content");
        fs::remove_file(root.join("stale-link")).unwrap();
        BackupService::restore_snapshot(&root, &snapshot.backup_file_path).unwrap();

        assert_eq!(
            fs::read_link(root.join("stale-link")).unwrap(),
            PathBuf::from("missing-target")
        );
        let _ = fs::remove_dir_all(root);
        let sidecar = snapshot.backup_file_path.with_file_name(format!(
            "{}.meta.json",
            snapshot
                .backup_file_path
                .file_name()
                .unwrap()
                .to_string_lossy()
        ));
        let _ = fs::remove_file(sidecar);
        let _ = fs::remove_file(snapshot.backup_file_path);
    }

    #[test]
    fn snapshots_for_multiple_locations_never_share_a_backup_file() {
        let first = std::env::temp_dir().join(format!(
            "skillsync-backup-first-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let second = std::env::temp_dir().join(format!(
            "skillsync-backup-second-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();

        let first_snapshot =
            BackupService::create_snapshot(&first, "same-resource", "1.0.0").unwrap();
        let second_snapshot =
            BackupService::create_snapshot(&second, "same-resource", "1.0.0").unwrap();

        assert_ne!(
            first_snapshot.backup_file_path,
            second_snapshot.backup_file_path
        );
        assert_ne!(first_snapshot.snapshot_id, second_snapshot.snapshot_id);
        for (root, snapshot) in [(&first, first_snapshot), (&second, second_snapshot)] {
            let _ = fs::remove_dir_all(root);
            let sidecar = snapshot.backup_file_path.with_file_name(format!(
                "{}.meta.json",
                snapshot
                    .backup_file_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
            ));
            let _ = fs::remove_file(sidecar);
            let _ = fs::remove_file(snapshot.backup_file_path);
        }
    }
}
