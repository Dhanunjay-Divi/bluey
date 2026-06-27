use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cue_core::app_paths::AppPaths;
use cue_core::MeetingRecord;

#[derive(Debug, Clone)]
pub struct MeetingStore {
    active_file: PathBuf,
    archive_dir: PathBuf,
    legacy_active_file: Option<PathBuf>,
    legacy_archive_dir: Option<PathBuf>,
}

impl MeetingStore {
    pub fn new(paths: &AppPaths) -> Result<Self> {
        let archive_dir = paths.data_dir.join("meetings");
        cue_core::app_paths::create_private_dir(&archive_dir)?;
        let legacy_dir = legacy_data_dir_for(&paths.data_dir);
        Ok(Self {
            active_file: paths.data_dir.join("active-meeting.json"),
            archive_dir,
            legacy_active_file: legacy_dir
                .as_ref()
                .map(|dir| dir.join("active-meeting.json")),
            legacy_archive_dir: legacy_dir.map(|dir| dir.join("meetings")),
        })
    }

    pub fn load_active(&self) -> Result<Option<MeetingRecord>> {
        if !self.active_file.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&self.active_file)
            .with_context(|| format!("failed to read {}", self.active_file.display()))?;
        let meeting = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", self.active_file.display()))?;
        Ok(Some(meeting))
    }

    pub fn save_active(&self, meeting: &MeetingRecord) -> Result<()> {
        write_private_json(&self.active_file, meeting)
    }

    pub fn archive(&self, meeting: &MeetingRecord) -> Result<PathBuf> {
        let filename = format!("{}-{}.json", meeting.started_at, meeting.id);
        let path = self.archive_dir.join(filename);
        write_private_json(&path, meeting)?;
        let _ = fs::remove_file(&self.active_file);
        Ok(path)
    }

    pub fn save_archived(&self, meeting: &MeetingRecord) -> Result<PathBuf> {
        let filename = format!("{}-{}.json", meeting.started_at, meeting.id);
        let path = self.archive_dir.join(filename);
        write_private_json(&path, meeting)?;
        Ok(path)
    }

    pub fn last_meeting(&self) -> Result<Option<MeetingRecord>> {
        Ok(self.all_meetings()?.into_iter().next())
    }

    pub fn all_meetings(&self) -> Result<Vec<MeetingRecord>> {
        let mut meetings = Vec::new();
        if let Some(active) = self.load_active()? {
            meetings.push(active);
        }
        if let Some(path) = self.legacy_active_file.as_ref() {
            if let Some(active) = read_optional_meeting(path)? {
                meetings.push(active);
            }
        }

        for archive_dir in self.archive_dirs() {
            meetings.extend(read_archived_meetings(archive_dir)?);
        }

        meetings.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        let mut seen = std::collections::HashSet::new();
        meetings.retain(|meeting| seen.insert(meeting.id));
        Ok(meetings)
    }

    pub fn load_by_id(&self, id: uuid::Uuid) -> Result<Option<MeetingRecord>> {
        if let Some(active) = self.load_active()? {
            if active.id == id {
                return Ok(Some(active));
            }
        }
        if let Some(path) = self.legacy_active_file.as_ref() {
            if let Some(active) = read_optional_meeting(path)? {
                if active.id == id {
                    return Ok(Some(active));
                }
            }
        }

        let Some(path) = self.archive_path_for(id)? else {
            return Ok(None);
        };
        self.read_meeting(&path).map(Some)
    }

    pub fn rename(&self, id: uuid::Uuid, title: &str) -> Result<MeetingRecord> {
        let title = title.trim();
        if title.is_empty() {
            anyhow::bail!("meeting title cannot be empty");
        }

        if let Some(mut active) = self.load_active()? {
            if active.id == id {
                active.title = title.to_string();
                self.save_active(&active)?;
                return Ok(active);
            }
        }
        if let Some(path) = self.legacy_active_file.as_ref() {
            if let Some(mut active) = read_optional_meeting(path)? {
                if active.id == id {
                    active.title = title.to_string();
                    write_private_json(path, &active)?;
                    return Ok(active);
                }
            }
        }

        let path = self
            .archive_path_for(id)?
            .with_context(|| format!("meeting {id} not found"))?;
        let mut meeting = self.read_meeting(&path)?;
        meeting.title = title.to_string();
        write_private_json(&path, &meeting)?;
        Ok(meeting)
    }

    pub fn delete(&self, id: uuid::Uuid) -> Result<bool> {
        let mut deleted = false;
        if let Some(active) = self.load_active()? {
            if active.id == id {
                fs::remove_file(&self.active_file)
                    .with_context(|| format!("failed to delete {}", self.active_file.display()))?;
                deleted = true;
            }
        }
        if let Some(path) = self.legacy_active_file.as_ref() {
            if let Some(active) = read_optional_meeting(path)? {
                if active.id == id {
                    fs::remove_file(path)
                        .with_context(|| format!("failed to delete {}", path.display()))?;
                    deleted = true;
                }
            }
        }

        if let Some(path) = self.archive_path_for(id)? {
            fs::remove_file(&path)
                .with_context(|| format!("failed to delete {}", path.display()))?;
            deleted = true;
        }

        Ok(deleted)
    }

    fn archive_path_for(&self, id: uuid::Uuid) -> Result<Option<PathBuf>> {
        for archive_dir in self.archive_dirs() {
            if !archive_dir.exists() {
                continue;
            }
            for entry in fs::read_dir(archive_dir)
                .with_context(|| format!("failed to read {}", archive_dir.display()))?
            {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                if path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| stem.ends_with(&id.to_string()))
                {
                    return Ok(Some(path));
                }
            }
        }
        Ok(None)
    }

    fn archive_dirs(&self) -> Vec<&Path> {
        let mut dirs = vec![self.archive_dir.as_path()];
        if let Some(dir) = self.legacy_archive_dir.as_deref() {
            dirs.push(dir);
        }
        dirs
    }

    fn read_meeting(&self, path: &Path) -> Result<MeetingRecord> {
        let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", path.display()))
    }
}

fn legacy_data_dir_for(data_dir: &Path) -> Option<PathBuf> {
    if data_dir.file_name().and_then(|name| name.to_str()) != Some("bluey") {
        return None;
    }
    let legacy = data_dir.parent()?.join("cue");
    legacy.exists().then_some(legacy)
}

fn read_optional_meeting(path: &Path) -> Result<Option<MeetingRecord>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let meeting = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(meeting))
}

fn read_archived_meetings(archive_dir: &Path) -> Result<Vec<MeetingRecord>> {
    if !archive_dir.exists() {
        return Ok(Vec::new());
    }

    let mut meetings = Vec::new();
    for entry in fs::read_dir(archive_dir)
        .with_context(|| format!("failed to read {}", archive_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        if let Some(meeting) = read_optional_meeting(&path)? {
            meetings.push(meeting);
        }
    }
    Ok(meetings)
}

fn write_private_json(path: &Path, meeting: &MeetingRecord) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(meeting)?;
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    set_private_file_permissions(path)
}

fn set_private_file_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set private permissions on {}", path.display()))?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}

#[cfg(test)]
mod security_tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn meeting_store_writes_private_files() {
        use std::os::unix::fs::PermissionsExt;

        let base = std::env::temp_dir().join(format!(
            "bluey-meeting-store-perms-{}",
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("run"),
            state_file: base.join("run/daemon-state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        };
        paths.ensure().expect("ensure paths");
        let store = MeetingStore::new(&paths).expect("store");
        let meeting = MeetingRecord::new(Some("Security permissions".to_string()));

        store.save_active(&meeting).expect("save active");
        let active_mode = fs::metadata(&store.active_file)
            .expect("active metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(active_mode, 0o600);

        let archive_path = store.archive(&meeting).expect("archive");
        let archive_mode = fs::metadata(&archive_path)
            .expect("archive metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(archive_mode, 0o600);

        let archive_dir_mode = fs::metadata(&store.archive_dir)
            .expect("archive dir metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(archive_dir_mode, 0o700);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn meeting_store_deletes_active_and_archived_meetings() {
        let base = std::env::temp_dir().join(format!(
            "bluey-meeting-store-delete-{}",
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("run"),
            state_file: base.join("run/daemon-state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        };
        paths.ensure().expect("ensure paths");
        let store = MeetingStore::new(&paths).expect("store");

        let active = MeetingRecord::new(Some("Active delete".to_string()));
        store.save_active(&active).expect("save active");
        assert!(store.delete(active.id).expect("delete active"));
        assert!(store.load_active().expect("load active").is_none());

        let archived = MeetingRecord::new(Some("Archived delete".to_string()));
        let archive_path = store.archive(&archived).expect("archive");
        assert!(archive_path.exists());
        assert!(store.delete(archived.id).expect("delete archived"));
        assert!(!archive_path.exists());
        assert!(!store
            .delete(uuid::Uuid::new_v4())
            .expect("delete missing meeting"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn meeting_store_reads_legacy_cue_history_from_bluey_store() {
        let base = std::env::temp_dir().join(format!(
            "bluey-meeting-store-legacy-{}",
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths {
            data_dir: base.join("bluey"),
            config_dir: base.join("config"),
            runtime_dir: base.join("run"),
            state_file: base.join("run/daemon-state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        };
        paths.ensure().expect("ensure paths");

        let legacy_paths = AppPaths {
            data_dir: base.join("cue"),
            config_dir: base.join("config-legacy"),
            runtime_dir: base.join("run-legacy"),
            state_file: base.join("run-legacy/daemon-state.json"),
            account_file: base.join("config-legacy/account.json"),
            settings_file: base.join("config-legacy/settings.json"),
        };
        legacy_paths.ensure().expect("ensure legacy paths");
        let legacy_store = MeetingStore::new(&legacy_paths).expect("legacy store");
        let mut legacy = MeetingRecord::new(Some("Legacy local recording".to_string()));
        legacy.transcript.push(cue_core::TranscriptSegment::new(
            cue_core::Speaker::User,
            "legacy transcript",
            true,
        ));
        let legacy_id = legacy.id;
        legacy_store
            .save_archived(&legacy)
            .expect("save legacy archived meeting");

        let store = MeetingStore::new(&paths).expect("bluey store");
        let meetings = store.all_meetings().expect("all meetings");
        assert!(meetings.iter().any(|meeting| meeting.id == legacy_id));
        assert_eq!(
            store
                .load_by_id(legacy_id)
                .expect("load by id")
                .expect("legacy meeting")
                .title,
            "Legacy local recording"
        );

        let _ = fs::remove_dir_all(base);
    }
}
