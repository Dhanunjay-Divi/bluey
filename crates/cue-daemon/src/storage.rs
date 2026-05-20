use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cue_core::app_paths::AppPaths;
use cue_core::MeetingRecord;

#[derive(Debug, Clone)]
pub struct MeetingStore {
    active_file: PathBuf,
    archive_dir: PathBuf,
}

impl MeetingStore {
    pub fn new(paths: &AppPaths) -> Result<Self> {
        let archive_dir = paths.data_dir.join("meetings");
        fs::create_dir_all(&archive_dir)
            .with_context(|| format!("failed to create {}", archive_dir.display()))?;
        Ok(Self {
            active_file: paths.data_dir.join("active-meeting.json"),
            archive_dir,
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
        let bytes = serde_json::to_vec_pretty(meeting)?;
        fs::write(&self.active_file, bytes)
            .with_context(|| format!("failed to write {}", self.active_file.display()))
    }

    pub fn archive(&self, meeting: &MeetingRecord) -> Result<PathBuf> {
        let filename = format!("{}-{}.json", meeting.started_at, meeting.id);
        let path = self.archive_dir.join(filename);
        let bytes = serde_json::to_vec_pretty(meeting)?;
        fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
        let _ = fs::remove_file(&self.active_file);
        Ok(path)
    }

    pub fn last_meeting(&self) -> Result<Option<MeetingRecord>> {
        if let Some(active) = self.load_active()? {
            return Ok(Some(active));
        }

        let mut candidates = Vec::new();
        for entry in fs::read_dir(&self.archive_dir)
            .with_context(|| format!("failed to read {}", self.archive_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                let modified = entry.metadata()?.modified()?;
                candidates.push((modified, path));
            }
        }

        candidates.sort_by_key(|(modified, _)| *modified);
        let Some((_, path)) = candidates.pop() else {
            return Ok(None);
        };

        let bytes =
            fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        let meeting = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Some(meeting))
    }

    pub fn all_meetings(&self) -> Result<Vec<MeetingRecord>> {
        let mut meetings = Vec::new();
        if let Some(active) = self.load_active()? {
            meetings.push(active);
        }

        if !self.archive_dir.exists() {
            return Ok(meetings);
        }

        for entry in fs::read_dir(&self.archive_dir)
            .with_context(|| format!("failed to read {}", self.archive_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }

            let bytes =
                fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
            let meeting = serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            meetings.push(meeting);
        }

        meetings.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        Ok(meetings)
    }

    pub fn load_by_id(&self, id: uuid::Uuid) -> Result<Option<MeetingRecord>> {
        if let Some(active) = self.load_active()? {
            if active.id == id {
                return Ok(Some(active));
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

        let path = self
            .archive_path_for(id)?
            .with_context(|| format!("meeting {id} not found"))?;
        let mut meeting = self.read_meeting(&path)?;
        meeting.title = title.to_string();
        let bytes = serde_json::to_vec_pretty(&meeting)?;
        fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(meeting)
    }

    fn archive_path_for(&self, id: uuid::Uuid) -> Result<Option<PathBuf>> {
        if !self.archive_dir.exists() {
            return Ok(None);
        }

        for entry in fs::read_dir(&self.archive_dir)
            .with_context(|| format!("failed to read {}", self.archive_dir.display()))?
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
        Ok(None)
    }

    fn read_meeting(&self, path: &Path) -> Result<MeetingRecord> {
        let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", path.display()))
    }
}
