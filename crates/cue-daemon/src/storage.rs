use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use cue_core::app_paths::AppPaths;
use cue_core::MeetingRecord;
use parking_lot::Mutex;

const STALE_TEMP_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const STALE_CORRUPT_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const CLEANUP_SCAN_LIMIT: usize = 512;
const CLEANUP_REMOVE_LIMIT: usize = 32;
const TEMP_FILE_PREFIX: &str = ".bluey-meeting-store-tmp-";
const MAX_MEETING_FILE_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

#[derive(Debug, Clone)]
pub struct MeetingStore {
    active_file: PathBuf,
    archive_dir: PathBuf,
    legacy_active_file: Option<PathBuf>,
    legacy_archive_dir: Option<PathBuf>,
    /// Serializes complete read/recovery/write transactions across every clone.
    /// Individual helpers are deliberately unlocked so public operations acquire
    /// this exactly once and cannot deadlock through nested store calls.
    operation_lock: Arc<Mutex<()>>,
}

impl MeetingStore {
    pub fn new(paths: &AppPaths) -> Result<Self> {
        let archive_dir = paths.data_dir.join("meetings");
        cue_core::app_paths::create_private_dir(&archive_dir)?;
        let legacy_dir = legacy_data_dir_for(&paths.data_dir);
        let store = Self {
            active_file: paths.data_dir.join("active-meeting.json"),
            archive_dir,
            legacy_active_file: legacy_dir
                .as_ref()
                .map(|dir| dir.join("active-meeting.json")),
            legacy_archive_dir: legacy_dir.map(|dir| dir.join("meetings")),
            operation_lock: Arc::new(Mutex::new(())),
        };
        {
            let _guard = store.operation_lock.lock();
            if let Err(error) = store.cleanup_stale_artifacts_unlocked() {
                tracing::warn!(error = %error, "meeting-store stale artifact cleanup failed");
            }
        }
        Ok(store)
    }

    pub fn load_active(&self) -> Result<Option<MeetingRecord>> {
        let _guard = self.operation_lock.lock();
        self.load_active_unlocked()
    }

    pub fn save_active(&self, meeting: &MeetingRecord) -> Result<()> {
        let _guard = self.operation_lock.lock();
        write_recoverable_private_json(&self.active_file, meeting)
    }

    pub fn archive(&self, meeting: &MeetingRecord) -> Result<PathBuf> {
        let _guard = self.operation_lock.lock();
        let filename = format!("{}-{}.json", meeting.started_at, meeting.id);
        let path = self.archive_dir.join(filename);
        write_private_json(&path, meeting)?;
        remove_file_if_exists(&self.active_file)?;
        remove_file_if_exists(&backup_path(&self.active_file))?;
        sync_directory(
            self.active_file
                .parent()
                .context("active meeting has no parent")?,
        )?;
        Ok(path)
    }

    pub fn save_archived(&self, meeting: &MeetingRecord) -> Result<PathBuf> {
        let _guard = self.operation_lock.lock();
        let filename = format!("{}-{}.json", meeting.started_at, meeting.id);
        let path = self.archive_dir.join(filename);
        write_private_json(&path, meeting)?;
        Ok(path)
    }

    pub fn last_meeting(&self) -> Result<Option<MeetingRecord>> {
        let _guard = self.operation_lock.lock();
        Ok(self.all_meetings_unlocked()?.into_iter().next())
    }

    pub fn all_meetings(&self) -> Result<Vec<MeetingRecord>> {
        let _guard = self.operation_lock.lock();
        self.all_meetings_unlocked()
    }

    pub fn load_by_id(&self, id: uuid::Uuid) -> Result<Option<MeetingRecord>> {
        let _guard = self.operation_lock.lock();
        self.load_by_id_unlocked(id)
    }

    pub fn rename(&self, id: uuid::Uuid, title: &str) -> Result<MeetingRecord> {
        let _guard = self.operation_lock.lock();
        self.rename_unlocked(id, title)
    }

    pub fn delete(&self, id: uuid::Uuid) -> Result<bool> {
        let _guard = self.operation_lock.lock();
        self.delete_unlocked(id)
    }

    fn load_active_unlocked(&self) -> Result<Option<MeetingRecord>> {
        let Some(bytes) = read_optional_private_bytes(&self.active_file)? else {
            return self.recover_active_from_backup_unlocked(None);
        };
        match serde_json::from_slice(&bytes) {
            Ok(meeting) => Ok(Some(meeting)),
            Err(error) => self.recover_active_from_backup_unlocked(Some((bytes, error))),
        }
    }

    fn all_meetings_unlocked(&self) -> Result<Vec<MeetingRecord>> {
        let mut meetings = Vec::new();
        if let Some(active) = self.load_active_unlocked()? {
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

    fn load_by_id_unlocked(&self, id: uuid::Uuid) -> Result<Option<MeetingRecord>> {
        if let Some(active) = self.load_active_unlocked()? {
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
        read_meeting(&path).map(Some)
    }

    fn rename_unlocked(&self, id: uuid::Uuid, title: &str) -> Result<MeetingRecord> {
        let title = title.trim();
        if title.is_empty() {
            anyhow::bail!("meeting title cannot be empty");
        }

        if let Some(mut active) = self.load_active_unlocked()? {
            if active.id == id {
                active.title = title.to_string();
                write_recoverable_private_json(&self.active_file, &active)?;
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
        let mut meeting = read_meeting(&path)?;
        meeting.title = title.to_string();
        write_private_json(&path, &meeting)?;
        Ok(meeting)
    }

    fn delete_unlocked(&self, id: uuid::Uuid) -> Result<bool> {
        let mut deleted = false;
        if let Some(active) = self.load_active_unlocked()? {
            if active.id == id {
                fs::remove_file(&self.active_file)
                    .with_context(|| format!("failed to delete {}", self.active_file.display()))?;
                remove_file_if_exists(&backup_path(&self.active_file))?;
                sync_directory(
                    self.active_file
                        .parent()
                        .context("active meeting has no parent")?,
                )?;
                deleted = true;
            }
        }
        if let Some(path) = self.legacy_active_file.as_ref() {
            if let Some(active) = read_optional_meeting(path)? {
                if active.id == id {
                    fs::remove_file(path)
                        .with_context(|| format!("failed to delete {}", path.display()))?;
                    remove_file_if_exists(&backup_path(path))?;
                    if let Some(parent) = path.parent() {
                        sync_directory(parent)?;
                    }
                    deleted = true;
                }
            }
        }

        if let Some(path) = self.archive_path_for(id)? {
            fs::remove_file(&path)
                .with_context(|| format!("failed to delete {}", path.display()))?;
            remove_file_if_exists(&backup_path(&path))?;
            if let Some(parent) = path.parent() {
                sync_directory(parent)?;
            }
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

    fn recover_active_from_backup_unlocked(
        &self,
        corrupt_primary: Option<(Vec<u8>, serde_json::Error)>,
    ) -> Result<Option<MeetingRecord>> {
        let backup = backup_path(&self.active_file);
        let Some(bytes) = read_optional_private_bytes(&backup)? else {
            return match corrupt_primary {
                Some((_, error)) => Err(error)
                    .with_context(|| format!("failed to parse {}", self.active_file.display())),
                None => Ok(None),
            };
        };
        let meeting: MeetingRecord = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse recovery file {}", backup.display()))?;

        if let Some((primary_bytes, _)) = corrupt_primary {
            let quarantine = corrupt_path(&self.active_file);
            // Publish a copy first. The corrupt primary remains present until the
            // verified backup atomically replaces it, so a crash cannot create a
            // missing-primary interval during recovery.
            write_private_bytes(&quarantine, &primary_bytes, false)?;
            tracing::warn!(
                path = %quarantine.display(),
                "quarantined corrupt active meeting before backup recovery"
            );
        }

        write_private_bytes(&self.active_file, &bytes, false)?;
        tracing::warn!(
            meeting_id = %meeting.id,
            "recovered active meeting from private backup"
        );
        Ok(Some(meeting))
    }

    fn cleanup_stale_artifacts_unlocked(&self) -> Result<CleanupStats> {
        let mut directories = vec![
            self.active_file.parent().map(Path::to_path_buf),
            Some(self.archive_dir.clone()),
            self.legacy_active_file
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf),
            self.legacy_archive_dir.clone(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        directories.sort();
        directories.dedup();
        cleanup_stale_artifacts(&directories, SystemTime::now(), jobs_now_ms())
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
    let Some(bytes) = read_optional_private_bytes(path)? else {
        return Ok(None);
    };
    let meeting = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(meeting))
}

fn read_meeting(path: &Path) -> Result<MeetingRecord> {
    let bytes = read_optional_private_bytes(path)?
        .with_context(|| format!("meeting file {} is missing", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn read_optional_private_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to open {}", path.display()))
        }
    };
    let metadata = file
        .metadata()
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if !metadata.is_file() {
        anyhow::bail!("{} is not a regular meeting file", path.display());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            anyhow::bail!("{} is a reparse-point meeting file", path.display());
        }
    }
    if metadata.len() > MAX_MEETING_FILE_BYTES {
        anyhow::bail!("{} exceeds the meeting file size limit", path.display());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::io::Read::by_ref(&mut file)
        .take(MAX_MEETING_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if bytes.len() as u64 > MAX_MEETING_FILE_BYTES {
        anyhow::bail!("{} exceeds the meeting file size limit", path.display());
    }
    Ok(Some(bytes))
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
    write_private_atomic_bytes(path, &bytes)
}

fn write_recoverable_private_json(path: &Path, meeting: &MeetingRecord) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(meeting)?;
    write_private_bytes(path, &bytes, true)
}

pub(crate) fn write_private_atomic_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    write_private_bytes(path, bytes, false)
}

fn write_private_bytes(path: &Path, bytes: &[u8], preserve_backup: bool) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    cue_core::app_paths::create_private_dir(parent)?;
    let temporary = stage_private_bytes(path, bytes)?;
    let publish = (|| {
        if preserve_backup {
            ensure_valid_backup(path, bytes)?;
        }
        atomic_replace_file(&temporary, path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        sync_directory(parent)
    })();
    if publish.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    publish
}

/// Before replacing a valid primary, atomically publish its exact bytes as the
/// single backup while leaving the primary continuously addressable. If the
/// primary is absent or corrupt, seed the backup with the new known-valid
/// meeting so a later recovery cannot resurrect an unrelated stale value.
fn ensure_valid_backup(path: &Path, new_bytes: &[u8]) -> Result<()> {
    let backup = backup_path(path);
    let primary_bytes = read_optional_private_bytes(path)?;
    let source = primary_bytes
        .as_deref()
        .filter(|bytes| valid_meeting_bytes(bytes))
        // A missing/corrupt primary cannot be a safe predecessor. Seed the
        // backup with the new intended value so no later reader can resurrect
        // an unrelated stale backup after this save commits.
        .unwrap_or(new_bytes);
    atomic_publish_private(&backup, source)
}

fn valid_meeting_bytes(bytes: &[u8]) -> bool {
    serde_json::from_slice::<MeetingRecord>(bytes).is_ok()
}

fn atomic_publish_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    let temporary = stage_private_bytes(path, bytes)?;
    let publish = atomic_replace_file(&temporary, path)
        .with_context(|| format!("failed to publish {}", path.display()))
        .and_then(|_| sync_directory(parent));
    if publish.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    publish
}

fn stage_private_bytes(path: &Path, bytes: &[u8]) -> Result<PathBuf> {
    if bytes.len() as u64 > MAX_MEETING_FILE_BYTES {
        anyhow::bail!("{} exceeds the meeting file size limit", path.display());
    }
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    cue_core::app_paths::create_private_dir(parent)?;
    let temporary = temporary_path(path);
    let write = (|| {
        let mut file = create_private_file(&temporary)?;
        file.write_all(bytes)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        set_private_file_permissions(&temporary)?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temporary.display()))?;
        Ok::<(), anyhow::Error>(())
    })();
    if write.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write.map(|_| temporary)
}

fn create_private_file(path: &Path) -> Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))
}

fn backup_path(path: &Path) -> PathBuf {
    path_with_suffix(path, ".bak")
}

fn temporary_path(path: &Path) -> PathBuf {
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(
            "{TEMP_FILE_PREFIX}{}-{}",
            jobs_now_ms(),
            uuid::Uuid::new_v4().simple()
        ))
}

fn corrupt_path(path: &Path) -> PathBuf {
    path_with_suffix(
        path,
        &format!(
            ".corrupt-{}-{}",
            jobs_now_ms(),
            uuid::Uuid::new_v4().simple()
        ),
    )
}

#[cfg(unix)]
fn atomic_replace_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    // POSIX rename within one directory atomically replaces the destination;
    // the old primary remains addressable until the replacement commits.
    fs::rename(temporary, path)
}

#[cfg(windows)]
fn atomic_replace_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let from = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let to = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let moved = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn atomic_replace_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(temporary, path)
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(suffix);
    PathBuf::from(value)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct CleanupStats {
    scanned: usize,
    removed: usize,
}

fn cleanup_stale_artifacts(
    directories: &[PathBuf],
    now: SystemTime,
    now_ms: i64,
) -> Result<CleanupStats> {
    let mut stats = CleanupStats::default();
    for directory in directories {
        if stats.scanned >= CLEANUP_SCAN_LIMIT || stats.removed >= CLEANUP_REMOVE_LIMIT {
            break;
        }
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to scan {}", directory.display()))
            }
        };
        let mut removed_from_directory = false;
        for entry in entries {
            if stats.scanned >= CLEANUP_SCAN_LIMIT || stats.removed >= CLEANUP_REMOVE_LIMIT {
                break;
            }
            stats.scanned += 1;
            let Ok(entry) = entry else {
                continue;
            };
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            // Never follow or remove a directory/symlink/reparse point. Removing
            // only exact task-owned regular-file names keeps cleanup contained.
            if !file_type.is_file() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !owned_artifact_is_stale(name, &metadata, now, now_ms) {
                continue;
            }
            match fs::remove_file(entry.path()) {
                Ok(()) => {
                    stats.removed += 1;
                    removed_from_directory = true;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(
                        path = %entry.path().display(),
                        error = %error,
                        "failed to remove stale meeting-store artifact"
                    );
                }
            }
        }
        if removed_from_directory {
            sync_directory(directory)?;
        }
    }
    Ok(stats)
}

fn owned_artifact_is_stale(
    name: &str,
    metadata: &fs::Metadata,
    now: SystemTime,
    now_ms: i64,
) -> bool {
    if let Some(suffix) = name.strip_prefix(TEMP_FILE_PREFIX) {
        return timestamped_uuid_age_ms(suffix, now_ms)
            .is_some_and(|age| age >= STALE_TEMP_AGE.as_millis() as i64);
    }
    if let Some(suffix) = name.strip_prefix("active-meeting.json.corrupt-") {
        let timestamp = suffix
            .parse::<i64>()
            .ok()
            .or_else(|| timestamped_uuid_timestamp(suffix));
        return timestamp
            .map(|timestamp| now_ms.saturating_sub(timestamp))
            .is_some_and(|age| age >= STALE_CORRUPT_AGE.as_millis() as i64);
    }
    // Safely retire legacy UUID-only temp names from the previous writer. The
    // base must itself be an exact active/archive meeting file (or its backup),
    // and age comes from metadata because those names carried no timestamp.
    if let Some((base, suffix)) = name.rsplit_once(".tmp-") {
        if uuid::Uuid::parse_str(suffix).is_ok()
            && is_owned_meeting_filename(base.strip_suffix(".bak").unwrap_or(base))
        {
            return metadata
                .modified()
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age >= STALE_TEMP_AGE);
        }
    }
    false
}

fn timestamped_uuid_age_ms(value: &str, now_ms: i64) -> Option<i64> {
    timestamped_uuid_timestamp(value).map(|timestamp| now_ms.saturating_sub(timestamp))
}

fn timestamped_uuid_timestamp(value: &str) -> Option<i64> {
    let (timestamp, id) = value.split_once('-')?;
    let timestamp = timestamp.parse::<i64>().ok().filter(|value| *value > 0)?;
    uuid::Uuid::parse_str(id).ok()?;
    Some(timestamp)
}

fn is_owned_meeting_filename(name: &str) -> bool {
    if name == "active-meeting.json" {
        return true;
    }
    let Some(stem) = name.strip_suffix(".json") else {
        return false;
    };
    if stem.len() <= 37 {
        return false;
    }
    let id_start = stem.len() - 36;
    stem.as_bytes().get(id_start.wrapping_sub(1)) == Some(&b'-')
        && stem[..id_start - 1]
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        && uuid::Uuid::parse_str(&stem[id_start..]).is_ok()
}

fn jobs_now_ms() -> i64 {
    cue_core::clock::now_epoch_ms_string()
        .parse::<i64>()
        .unwrap_or_default()
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)
        .with_context(|| format!("failed to open directory {} for sync", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync directory {}", path.display()))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
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

    fn test_store(label: &str) -> (PathBuf, AppPaths, MeetingStore) {
        let base = std::env::temp_dir().join(format!(
            "bluey-meeting-store-{label}-{}",
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
        (base, paths, store)
    }

    #[test]
    fn meeting_store_clones_share_one_operation_lock() {
        let (base, _paths, store) = test_store("clone-lock");
        let cloned = store.clone();

        assert!(Arc::ptr_eq(&store.operation_lock, &cloned.operation_lock));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn private_state_publication_atomically_replaces_complete_payloads() {
        let (base, paths, _store) = test_store("atomic-state");
        let first = br#"{"generation":1,"status":"starting"}"#;
        let second = br#"{"generation":2,"status":"ready"}"#;

        write_private_atomic_bytes(&paths.state_file, first).expect("publish first state");
        assert_eq!(
            fs::read(&paths.state_file).expect("read first state"),
            first
        );

        write_private_atomic_bytes(&paths.state_file, second).expect("publish second state");
        assert_eq!(
            fs::read(&paths.state_file).expect("read second state"),
            second
        );
        assert!(fs::read_dir(&paths.runtime_dir)
            .expect("read runtime dir")
            .all(|entry| !entry
                .expect("runtime entry")
                .file_name()
                .to_string_lossy()
                .starts_with(TEMP_FILE_PREFIX)));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn cloned_store_concurrent_saves_and_reads_never_observe_partial_state() {
        use std::sync::Barrier;
        use std::thread;

        let (base, _paths, store) = test_store("concurrent-read-save");
        let mut initial = MeetingRecord::new(Some("value-0".to_string()));
        let meeting_id = initial.id;
        store.save_active(&initial).expect("initial save");

        let reader_count = 4;
        let barrier = Arc::new(Barrier::new(reader_count + 2));
        let writer_store = store.clone();
        let writer_barrier = barrier.clone();
        let writer = thread::spawn(move || {
            writer_barrier.wait();
            for index in 1..=150 {
                initial.title = format!("value-{index}");
                writer_store.save_active(&initial).expect("concurrent save");
            }
        });

        let mut readers = Vec::new();
        for _ in 0..reader_count {
            let reader_store = store.clone();
            let reader_barrier = barrier.clone();
            readers.push(thread::spawn(move || {
                reader_barrier.wait();
                for _ in 0..300 {
                    let observed = reader_store
                        .load_active()
                        .expect("concurrent read")
                        .expect("primary never disappears");
                    assert_eq!(observed.id, meeting_id);
                    assert!(observed.title.starts_with("value-"));
                }
            }));
        }
        barrier.wait();
        writer.join().expect("writer thread");
        for reader in readers {
            reader.join().expect("reader thread");
        }
        assert_eq!(
            store
                .load_active()
                .expect("final read")
                .expect("final meeting")
                .title,
            "value-150"
        );
        assert!(valid_meeting_bytes(
            &read_optional_private_bytes(&backup_path(&store.active_file))
                .expect("read backup")
                .expect("backup exists")
        ));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn recovery_racing_a_newer_save_cannot_overwrite_the_new_primary() {
        use std::sync::Barrier;
        use std::thread;

        let (base, _paths, store) = test_store("recovery-save-race");
        let mut meeting = MeetingRecord::new(Some("old-backup".to_string()));
        store.save_active(&meeting).expect("old save");
        meeting.title = "corrupted-primary-version".to_string();
        store.save_active(&meeting).expect("second save");
        fs::write(&store.active_file, b"{corrupt primary").expect("corrupt primary");

        let barrier = Arc::new(Barrier::new(3));
        let writer_store = store.clone();
        let writer_barrier = barrier.clone();
        let mut newest = meeting.clone();
        newest.title = "newest-save".to_string();
        let writer = thread::spawn(move || {
            writer_barrier.wait();
            writer_store.save_active(&newest).expect("newer save");
        });
        let reader_store = store.clone();
        let reader_barrier = barrier.clone();
        let reader = thread::spawn(move || {
            reader_barrier.wait();
            let title = reader_store
                .load_active()
                .expect("racing recovery")
                .expect("racing meeting")
                .title;
            assert!(title == "old-backup" || title == "newest-save");
        });
        barrier.wait();
        writer.join().expect("writer thread");
        reader.join().expect("reader thread");

        assert_eq!(
            store
                .load_active()
                .expect("final read")
                .expect("final meeting")
                .title,
            "newest-save"
        );
        assert!(valid_meeting_bytes(
            &read_optional_private_bytes(&backup_path(&store.active_file))
                .expect("read backup")
                .expect("backup exists")
        ));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn crash_state_never_promotes_an_uncommitted_temp() {
        let (base, _paths, store) = test_store("uncommitted-temp");
        let stable = MeetingRecord::new(Some("stable-primary".to_string()));
        store.save_active(&stable).expect("stable save");
        let mut uncommitted = stable.clone();
        uncommitted.title = "uncommitted-temp".to_string();
        let bytes = serde_json::to_vec_pretty(&uncommitted).expect("serialize temp");
        let temporary = stage_private_bytes(&store.active_file, &bytes).expect("stage temp");

        assert_eq!(
            store
                .load_active()
                .expect("load stable primary")
                .expect("stable meeting")
                .title,
            "stable-primary"
        );
        assert!(temporary.exists());

        fs::remove_file(&store.active_file).expect("remove primary");
        fs::write(backup_path(&store.active_file), b"{corrupt backup").expect("corrupt backup");
        assert!(store.load_active().is_err());
        assert!(!store.active_file.exists());
        assert!(temporary.exists());

        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn meeting_store_refuses_to_follow_a_symlinked_primary() {
        use std::os::unix::fs::symlink;

        let (base, paths, store) = test_store("nofollow-primary");
        let external = base.join("external-meeting.json");
        let meeting = MeetingRecord::new(Some("must remain external".to_string()));
        let external_bytes = serde_json::to_vec_pretty(&meeting).expect("serialize external");
        fs::write(&external, &external_bytes).expect("write external target");
        symlink(&external, &store.active_file).expect("symlink active primary");

        assert!(store.load_active().is_err());
        assert_eq!(
            fs::read(&external).expect("read external target"),
            external_bytes
        );
        assert!(!backup_path(&store.active_file).exists());
        assert!(paths.data_dir.exists());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn meeting_store_rejects_oversized_primary_before_reading_it() {
        let (base, _paths, store) = test_store("oversized-primary");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&store.active_file)
            .expect("create sparse oversized primary");
        file.set_len(MAX_MEETING_FILE_BYTES + 1)
            .expect("extend sparse primary");

        let error = store.load_active().expect_err("oversized file must fail");
        assert!(error.to_string().contains("size limit"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn stale_cleanup_is_exact_bounded_and_never_follows_symlinks() {
        let (base, paths, store) = test_store("bounded-cleanup");
        let now_ms = jobs_now_ms();
        let stale_temp_ms = now_ms - STALE_TEMP_AGE.as_millis() as i64 - 1;
        let stale_corrupt_ms = now_ms - STALE_CORRUPT_AGE.as_millis() as i64 - 1;
        let recent_temp = paths.data_dir.join(format!(
            "{TEMP_FILE_PREFIX}{now_ms}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let stale_corrupt = paths.data_dir.join(format!(
            "active-meeting.json.corrupt-{stale_corrupt_ms}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let malformed = paths.data_dir.join(".bluey-meeting-store-tmp-not-owned");
        fs::write(&recent_temp, b"recent").expect("recent temp");
        fs::write(&stale_corrupt, b"old corrupt").expect("stale corrupt");
        fs::write(&malformed, b"malformed").expect("malformed file");

        let mut stale_temps = Vec::new();
        for _ in 0..(CLEANUP_REMOVE_LIMIT + 5) {
            let path = paths.data_dir.join(format!(
                "{TEMP_FILE_PREFIX}{stale_temp_ms}-{}",
                uuid::Uuid::new_v4().simple()
            ));
            fs::write(&path, b"stale").expect("stale temp");
            stale_temps.push(path);
        }

        #[cfg(unix)]
        let symlink = {
            use std::os::unix::fs::symlink;
            let target = paths.data_dir.join("must-survive.txt");
            fs::write(&target, b"keep").expect("symlink target");
            let link = paths.data_dir.join(format!(
                "{TEMP_FILE_PREFIX}{stale_temp_ms}-{}",
                uuid::Uuid::new_v4().simple()
            ));
            symlink(&target, &link).expect("stale-name symlink");
            (target, link)
        };

        let first = {
            let _guard = store.operation_lock.lock();
            store
                .cleanup_stale_artifacts_unlocked()
                .expect("first cleanup")
        };
        assert_eq!(first.removed, CLEANUP_REMOVE_LIMIT);
        assert!(recent_temp.exists());
        assert!(malformed.exists());
        #[cfg(unix)]
        {
            assert!(symlink.0.exists());
            assert!(symlink.1.symlink_metadata().is_ok());
        }

        let second = {
            let _guard = store.operation_lock.lock();
            store
                .cleanup_stale_artifacts_unlocked()
                .expect("second cleanup")
        };
        assert!(second.removed <= CLEANUP_REMOVE_LIMIT);
        assert!(!stale_corrupt.exists());
        assert!(stale_temps.iter().all(|path| !path.exists()));

        let _ = fs::remove_dir_all(base);
    }

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
        assert!(!backup_path(&archive_path).exists());

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
    fn meeting_store_recovers_a_valid_private_backup_and_quarantines_corruption() {
        let base = std::env::temp_dir().join(format!(
            "bluey-meeting-store-recovery-{}",
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

        let mut meeting = MeetingRecord::new(Some("first durable value".to_string()));
        store.save_active(&meeting).expect("first save");
        meeting.title = "newest value".to_string();
        store.save_active(&meeting).expect("second save");
        assert!(backup_path(&store.active_file).exists());

        fs::write(&store.active_file, b"{not valid json").expect("corrupt primary for test");
        let recovered = store
            .load_active()
            .expect("recover active")
            .expect("recovered meeting");
        assert_eq!(recovered.id, meeting.id);
        assert_eq!(recovered.title, "first durable value");
        assert_eq!(
            store
                .load_active()
                .expect("restored primary is readable")
                .expect("active meeting")
                .title,
            "first durable value"
        );
        assert!(fs::read_dir(&paths.data_dir)
            .expect("read data dir")
            .filter_map(Result::ok)
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("active-meeting.json.corrupt-")));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn meeting_store_recovers_interrupted_rotation_and_archive_clears_backup() {
        let base = std::env::temp_dir().join(format!(
            "bluey-meeting-store-interrupted-{}",
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
        let meeting = MeetingRecord::new(Some("recover interrupted write".to_string()));
        store.save_active(&meeting).expect("save active");

        let backup = backup_path(&store.active_file);
        remove_file_if_exists(&backup).expect("remove current backup");
        fs::rename(&store.active_file, &backup).expect("simulate interrupted rotation");
        assert!(!store.active_file.exists());
        assert_eq!(
            store
                .load_active()
                .expect("recover backup")
                .expect("active meeting")
                .id,
            meeting.id
        );

        let mut updated = meeting.clone();
        updated.title = "create current backup".to_string();
        store.save_active(&updated).expect("rotate active");
        assert!(backup.exists());
        store.archive(&updated).expect("archive");
        assert!(!store.active_file.exists());
        assert!(!backup.exists());
        assert!(store
            .load_active()
            .expect("no resurrected meeting")
            .is_none());

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
