use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use cue_core::app_paths::AppPaths;
#[cfg(test)]
use cue_core::AssistantProfile;
use cue_core::{
    MeetingRecord, WorkspaceCreateRequest, WorkspaceDeletionState, WorkspaceInstructionsPatch,
    WorkspaceLinkedJobMetadata, WorkspaceRecord, WorkspaceUpdateRequest,
};
use parking_lot::Mutex;

const ACTIVE_POINTER_FILE_NAME: &str = "active-workspace.json";
const TEMP_FILE_PREFIX: &str = ".bluey-workspace-tmp-";
const MAX_WORKSPACE_FILE_BYTES: u64 = 512 * 1024;
const MAX_WORKSPACE_RECORDS: usize = 512;
const MAX_ACTIVE_WORKSPACES_PER_OWNER: usize = 128;
const MAX_RETAINED_DELETED_WORKSPACES: usize = 256;
const CLEANUP_SCAN_LIMIT: usize = 512;
const CLEANUP_REMOVE_LIMIT: usize = 32;
#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

#[derive(Debug, Clone)]
pub struct WorkspaceStore {
    directory: PathBuf,
    active_pointer_file: PathBuf,
    operation_lock: Arc<Mutex<()>>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceDeleteOutcome {
    pub deleted: bool,
    pub active_workspace_id: uuid::Uuid,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ActiveWorkspacePointer {
    schema_version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner_account_id: Option<String>,
    workspace_id: uuid::Uuid,
}

impl WorkspaceStore {
    pub fn new(paths: &AppPaths) -> Result<Self> {
        let directory = paths.data_dir.join("workspaces");
        cue_core::app_paths::create_private_dir(&directory)?;
        let store = Self {
            active_pointer_file: directory.join(ACTIVE_POINTER_FILE_NAME),
            directory,
            operation_lock: Arc::new(Mutex::new(())),
        };
        {
            let _guard = store.operation_lock.lock();
            store.cleanup_stale_temps_unlocked()?;
        }
        Ok(store)
    }

    /// Creates one owner-scoped default exactly once. Existing workspaces win;
    /// the active meeting is used only for the first migration and is never
    /// copied into a second workspace on retry.
    pub fn migrate_default(
        &self,
        owner_account_id: Option<&str>,
        active_meeting: Option<&MeetingRecord>,
    ) -> Result<WorkspaceRecord> {
        let _guard = self.operation_lock.lock();
        let mut workspaces = self.list_for_owner_unlocked(owner_account_id, false)?;
        if workspaces.is_empty() {
            self.ensure_record_capacity_unlocked(owner_account_id)?;
            let profile = active_meeting
                .map(|meeting| meeting.assistant_profile.clone())
                .unwrap_or_default();
            let instructions =
                active_meeting.and_then(|meeting| meeting.answer_instructions.clone());
            let mut workspace = WorkspaceRecord::new(
                owner_account_id.map(ToString::to_string),
                "Default workspace".to_string(),
                profile,
                instructions,
            )
            .map_err(anyhow::Error::msg)?;
            if let Some(meeting) = active_meeting {
                workspace.linked_job = linked_job_from_meeting(meeting);
                workspace.validate().map_err(anyhow::Error::msg)?;
            }
            self.write_workspace_unlocked(&workspace)?;
            self.write_active_pointer_unlocked(owner_account_id, workspace.id)?;
            return Ok(workspace);
        }

        workspaces.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        if let Some(active) = self.active_for_owner_unlocked(owner_account_id)? {
            return Ok(active);
        }
        let workspace = workspaces.remove(0);
        self.write_active_pointer_unlocked(owner_account_id, workspace.id)?;
        Ok(workspace)
    }

    pub fn list_for_owner(&self, owner_account_id: Option<&str>) -> Result<Vec<WorkspaceRecord>> {
        let _guard = self.operation_lock.lock();
        self.list_for_owner_unlocked(owner_account_id, false)
    }

    pub fn get_for_owner(
        &self,
        owner_account_id: Option<&str>,
        workspace_id: uuid::Uuid,
    ) -> Result<Option<WorkspaceRecord>> {
        let _guard = self.operation_lock.lock();
        self.get_for_owner_unlocked(owner_account_id, workspace_id, false)
    }

    pub fn active_for_owner(
        &self,
        owner_account_id: Option<&str>,
    ) -> Result<Option<WorkspaceRecord>> {
        let _guard = self.operation_lock.lock();
        self.active_for_owner_unlocked(owner_account_id)
    }

    pub fn create(
        &self,
        owner_account_id: Option<&str>,
        request: WorkspaceCreateRequest,
    ) -> Result<WorkspaceRecord> {
        let _guard = self.operation_lock.lock();
        self.ensure_record_capacity_unlocked(owner_account_id)?;
        if self.list_for_owner_unlocked(owner_account_id, false)?.len()
            >= MAX_ACTIVE_WORKSPACES_PER_OWNER
        {
            return Err(anyhow!(
                "an account may contain at most {MAX_ACTIVE_WORKSPACES_PER_OWNER} active workspaces"
            ));
        }
        if request
            .profile
            .as_ref()
            .and_then(|profile| profile.source.as_ref())
            .is_some()
        {
            return Err(anyhow!(
                "linked Jobs provenance can only be added by a verified Jobs import"
            ));
        }
        let workspace = WorkspaceRecord::new(
            owner_account_id.map(ToString::to_string),
            request.title,
            request.profile.unwrap_or_default(),
            request.instructions,
        )
        .map_err(anyhow::Error::msg)?;
        self.write_workspace_unlocked(&workspace)?;
        Ok(workspace)
    }

    pub fn update(
        &self,
        owner_account_id: Option<&str>,
        request: WorkspaceUpdateRequest,
    ) -> Result<WorkspaceRecord> {
        let _guard = self.operation_lock.lock();
        let mut workspace = self
            .get_for_owner_unlocked(owner_account_id, request.workspace_id, false)?
            .with_context(|| format!("workspace {} not found", request.workspace_id))?;
        if workspace.revision != request.expected_revision {
            return Err(anyhow!(
                "workspace changed in another view; reload before saving"
            ));
        }
        if let Some(title) = request.title {
            workspace.title = title;
        }
        if let Some(profile) = request.profile {
            if profile.source != workspace.profile.source {
                return Err(anyhow!(
                    "workspace changed in another view; reload before saving"
                ));
            }
            workspace.profile = profile;
        }
        match request.instructions {
            WorkspaceInstructionsPatch::Unchanged => {}
            WorkspaceInstructionsPatch::Clear => workspace.instructions = None,
            WorkspaceInstructionsPatch::Set { text } => workspace.instructions = Some(text),
        }
        workspace
            .normalize_editable_fields()
            .map_err(anyhow::Error::msg)?;
        workspace.touch().map_err(anyhow::Error::msg)?;
        workspace.validate().map_err(anyhow::Error::msg)?;
        self.write_workspace_unlocked(&workspace)?;
        Ok(workspace)
    }

    pub fn activate(
        &self,
        owner_account_id: Option<&str>,
        workspace_id: uuid::Uuid,
    ) -> Result<WorkspaceRecord> {
        let _guard = self.operation_lock.lock();
        let workspace = self
            .get_for_owner_unlocked(owner_account_id, workspace_id, false)?
            .with_context(|| format!("workspace {workspace_id} not found"))?;
        self.write_active_pointer_unlocked(owner_account_id, workspace.id)?;
        Ok(workspace)
    }

    /// Soft deletion preserves meeting files and the workspace id on historic
    /// meetings. If the deleted workspace was active, the replacement pointer
    /// is durably published before the old workspace is marked deleted.
    pub fn delete(
        &self,
        owner_account_id: Option<&str>,
        workspace_id: uuid::Uuid,
        expected_revision: u64,
    ) -> Result<WorkspaceDeleteOutcome> {
        let _guard = self.operation_lock.lock();
        let Some(mut workspace) =
            self.get_for_owner_unlocked(owner_account_id, workspace_id, true)?
        else {
            let active = self.ensure_active_workspace_unlocked(owner_account_id, None)?;
            return Ok(WorkspaceDeleteOutcome {
                deleted: false,
                active_workspace_id: active.id,
            });
        };
        if !workspace.is_active() {
            let active = self.ensure_active_workspace_unlocked(owner_account_id, None)?;
            return Ok(WorkspaceDeleteOutcome {
                deleted: false,
                active_workspace_id: active.id,
            });
        }
        if workspace.revision != expected_revision {
            return Err(anyhow!(
                "workspace changed in another view; reload before deleting"
            ));
        }

        let current_active_id = self
            .active_for_owner_unlocked(owner_account_id)?
            .map(|active| active.id);
        let replacement = if current_active_id.is_none() || current_active_id == Some(workspace_id)
        {
            let replacement = self
                .list_for_owner_unlocked(owner_account_id, false)?
                .into_iter()
                .filter(|candidate| candidate.id != workspace_id)
                .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
                .map(Ok)
                .unwrap_or_else(|| self.create_default_unlocked(owner_account_id, None))?;
            self.write_active_pointer_unlocked(owner_account_id, replacement.id)?;
            replacement
        } else {
            self.ensure_active_workspace_unlocked(owner_account_id, None)?
        };

        workspace.deletion_state = WorkspaceDeletionState::Deleted {
            deleted_at: cue_core::clock::now_epoch_ms_string(),
        };
        workspace.touch().map_err(anyhow::Error::msg)?;
        workspace.validate().map_err(anyhow::Error::msg)?;
        self.write_workspace_unlocked(&workspace)?;
        Ok(WorkspaceDeleteOutcome {
            deleted: true,
            active_workspace_id: replacement.id,
        })
    }

    /// Keeps legacy AssistantProfile/Instructions/Jobs mutations coherent with
    /// the active named workspace without moving any session payload bytes.
    pub fn sync_active_settings_from_meeting(
        &self,
        owner_account_id: Option<&str>,
        meeting: &MeetingRecord,
    ) -> Result<WorkspaceRecord> {
        let _guard = self.operation_lock.lock();
        ensure_owner_matches(owner_account_id, meeting.owner_account_id.as_deref())?;
        let linked_workspace = if let Some(workspace_id) = meeting.workspace_id {
            self.get_for_owner_unlocked(owner_account_id, workspace_id, false)?
        } else {
            None
        };
        let mut workspace = if let Some(workspace) = linked_workspace {
            workspace
        } else if let Some(workspace) = self.active_for_owner_unlocked(owner_account_id)? {
            workspace
        } else {
            self.create_default_unlocked(owner_account_id, Some(meeting))?
        };

        let linked_job = linked_job_from_meeting(meeting).or_else(|| {
            (meeting.assistant_profile.source == workspace.profile.source)
                .then(|| workspace.linked_job.clone())
                .flatten()
        });
        if workspace.profile != meeting.assistant_profile
            || workspace.instructions != meeting.answer_instructions
            || !linked_jobs_equal(workspace.linked_job.as_ref(), linked_job.as_ref())
        {
            workspace.profile = meeting.assistant_profile.clone();
            workspace.instructions = meeting.answer_instructions.clone();
            workspace.linked_job = linked_job;
            workspace
                .normalize_editable_fields()
                .map_err(anyhow::Error::msg)?;
            workspace.touch().map_err(anyhow::Error::msg)?;
            workspace.validate().map_err(anyhow::Error::msg)?;
            self.write_workspace_unlocked(&workspace)?;
        }
        if self
            .active_for_owner_unlocked(owner_account_id)?
            .is_none_or(|active| active.id != workspace.id)
        {
            self.write_active_pointer_unlocked(owner_account_id, workspace.id)?;
        }
        Ok(workspace)
    }

    fn create_default_unlocked(
        &self,
        owner_account_id: Option<&str>,
        meeting: Option<&MeetingRecord>,
    ) -> Result<WorkspaceRecord> {
        self.ensure_record_capacity_unlocked(owner_account_id)?;
        let mut workspace = WorkspaceRecord::new(
            owner_account_id.map(ToString::to_string),
            "Default workspace".to_string(),
            meeting
                .map(|meeting| meeting.assistant_profile.clone())
                .unwrap_or_default(),
            meeting.and_then(|meeting| meeting.answer_instructions.clone()),
        )
        .map_err(anyhow::Error::msg)?;
        workspace.linked_job = meeting.and_then(linked_job_from_meeting);
        workspace.validate().map_err(anyhow::Error::msg)?;
        self.write_workspace_unlocked(&workspace)?;
        Ok(workspace)
    }

    fn ensure_active_workspace_unlocked(
        &self,
        owner_account_id: Option<&str>,
        meeting: Option<&MeetingRecord>,
    ) -> Result<WorkspaceRecord> {
        if let Some(active) = self.active_for_owner_unlocked(owner_account_id)? {
            return Ok(active);
        }
        let workspace = self
            .list_for_owner_unlocked(owner_account_id, false)?
            .into_iter()
            .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
            .map(Ok)
            .unwrap_or_else(|| self.create_default_unlocked(owner_account_id, meeting))?;
        self.write_active_pointer_unlocked(owner_account_id, workspace.id)?;
        Ok(workspace)
    }

    fn active_for_owner_unlocked(
        &self,
        owner_account_id: Option<&str>,
    ) -> Result<Option<WorkspaceRecord>> {
        let Some(pointer) = self.read_active_pointer_unlocked()? else {
            return Ok(None);
        };
        if !owners_match(pointer.owner_account_id.as_deref(), owner_account_id) {
            return Ok(None);
        }
        self.get_for_owner_unlocked(owner_account_id, pointer.workspace_id, false)
    }

    fn list_for_owner_unlocked(
        &self,
        owner_account_id: Option<&str>,
        include_deleted: bool,
    ) -> Result<Vec<WorkspaceRecord>> {
        let mut workspaces = Vec::new();
        let mut recognized = 0usize;
        for entry in fs::read_dir(&self.directory)
            .with_context(|| format!("failed to read {}", self.directory.display()))?
        {
            let entry = entry?;
            let Some(id) = workspace_id_from_file_name(&entry.file_name()) else {
                continue;
            };
            recognized += 1;
            if recognized > MAX_WORKSPACE_RECORDS {
                return Err(anyhow!("workspace record limit exceeded"));
            }
            let workspace = match self.read_workspace_unlocked(id) {
                Ok(Some(workspace)) => workspace,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %id,
                        error = %error,
                        "ignoring unreadable workspace record"
                    );
                    continue;
                }
            };
            if !owners_match(workspace.owner_account_id.as_deref(), owner_account_id) {
                continue;
            }
            if include_deleted || workspace.is_active() {
                workspaces.push(workspace);
            }
        }
        workspaces.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(workspaces)
    }

    fn get_for_owner_unlocked(
        &self,
        owner_account_id: Option<&str>,
        workspace_id: uuid::Uuid,
        include_deleted: bool,
    ) -> Result<Option<WorkspaceRecord>> {
        let Some(workspace) = self.read_workspace_unlocked(workspace_id)? else {
            return Ok(None);
        };
        if !owners_match(workspace.owner_account_id.as_deref(), owner_account_id) {
            return Err(anyhow!("workspace does not belong to the current account"));
        }
        if !include_deleted && !workspace.is_active() {
            return Ok(None);
        }
        Ok(Some(workspace))
    }

    fn read_workspace_unlocked(&self, workspace_id: uuid::Uuid) -> Result<Option<WorkspaceRecord>> {
        let path = self.workspace_path(workspace_id);
        let Some(bytes) = read_optional_private_bytes(&path)? else {
            return Ok(None);
        };
        let workspace: WorkspaceRecord = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        if workspace.id != workspace_id {
            return Err(anyhow!("workspace file identity mismatch"));
        }
        workspace.validate().map_err(anyhow::Error::msg)?;
        Ok(Some(workspace))
    }

    fn write_workspace_unlocked(&self, workspace: &WorkspaceRecord) -> Result<()> {
        workspace.validate().map_err(anyhow::Error::msg)?;
        write_private_json(&self.workspace_path(workspace.id), workspace)
    }

    fn read_active_pointer_unlocked(&self) -> Result<Option<ActiveWorkspacePointer>> {
        let Some(bytes) = read_optional_private_bytes(&self.active_pointer_file)? else {
            return Ok(None);
        };
        let pointer: ActiveWorkspacePointer = match serde_json::from_slice(&bytes) {
            Ok(pointer) => pointer,
            Err(error) => {
                tracing::warn!(
                    path = %self.active_pointer_file.display(),
                    error = %error,
                    "ignoring unreadable active workspace pointer"
                );
                return Ok(None);
            }
        };
        if pointer.schema_version != 1 {
            tracing::warn!(
                schema_version = pointer.schema_version,
                "ignoring unsupported active workspace pointer"
            );
            return Ok(None);
        }
        Ok(Some(pointer))
    }

    fn write_active_pointer_unlocked(
        &self,
        owner_account_id: Option<&str>,
        workspace_id: uuid::Uuid,
    ) -> Result<()> {
        let pointer = ActiveWorkspacePointer {
            schema_version: 1,
            owner_account_id: owner_account_id.map(ToString::to_string),
            workspace_id,
        };
        write_private_json(&self.active_pointer_file, &pointer)
    }

    fn workspace_path(&self, workspace_id: uuid::Uuid) -> PathBuf {
        self.directory.join(format!("{workspace_id}.json"))
    }

    fn cleanup_stale_temps_unlocked(&self) -> Result<()> {
        let mut removed = 0usize;
        for (scanned, entry) in fs::read_dir(&self.directory)
            .with_context(|| format!("failed to scan {}", self.directory.display()))?
            .enumerate()
        {
            if scanned >= CLEANUP_SCAN_LIMIT || removed >= CLEANUP_REMOVE_LIMIT {
                break;
            }
            let Ok(entry) = entry else {
                continue;
            };
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file()
                || !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(TEMP_FILE_PREFIX)
            {
                continue;
            }
            if fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        if removed > 0 {
            sync_directory(&self.directory)?;
        }
        Ok(())
    }

    /// Retains a bounded recent soft-delete window for idempotent retries.
    /// Compaction removes only workspace metadata; MeetingRecord files and
    /// their workspace ids remain untouched.
    fn ensure_record_capacity_unlocked(&self, owner_account_id: Option<&str>) -> Result<()> {
        let mut total = 0usize;
        let mut tombstones = Vec::new();
        for entry in fs::read_dir(&self.directory)
            .with_context(|| format!("failed to scan {}", self.directory.display()))?
        {
            let entry = entry?;
            let Some(id) = workspace_id_from_file_name(&entry.file_name()) else {
                continue;
            };
            total += 1;
            if let Ok(Some(workspace)) = self.read_workspace_unlocked(id) {
                if !workspace.is_active()
                    && owners_match(workspace.owner_account_id.as_deref(), owner_account_id)
                {
                    tombstones.push((workspace.updated_at, entry.path()));
                }
            }
        }

        tombstones.sort_by(|left, right| left.0.cmp(&right.0));
        let retention_excess = tombstones
            .len()
            .saturating_sub(MAX_RETAINED_DELETED_WORKSPACES);
        let capacity_excess = total.saturating_sub(MAX_WORKSPACE_RECORDS.saturating_sub(1));
        let remove_count = retention_excess.max(capacity_excess).min(tombstones.len());
        for (_, path) in tombstones.into_iter().take(remove_count) {
            match fs::remove_file(&path) {
                Ok(()) => total = total.saturating_sub(1),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    total = total.saturating_sub(1)
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to compact {}", path.display()))
                }
            }
        }
        if remove_count > 0 {
            sync_directory(&self.directory)?;
        }
        if total >= MAX_WORKSPACE_RECORDS {
            return Err(anyhow!(
                "workspace storage is full; delete unused workspaces before creating another"
            ));
        }
        Ok(())
    }
}

fn linked_job_from_meeting(meeting: &MeetingRecord) -> Option<WorkspaceLinkedJobMetadata> {
    Some(WorkspaceLinkedJobMetadata {
        import_id: meeting.jobs_handoff_import_id.clone()?,
        context_sha256: meeting.jobs_handoff_context_sha256.clone()?,
        source: meeting.assistant_profile.source.clone()?,
        linked_at: meeting.started_at.clone(),
    })
}

fn linked_jobs_equal(
    left: Option<&WorkspaceLinkedJobMetadata>,
    right: Option<&WorkspaceLinkedJobMetadata>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.import_id == right.import_id
                && left.context_sha256 == right.context_sha256
                && left.source == right.source
                && left.linked_at == right.linked_at
        }
        _ => false,
    }
}

fn owners_match(left: Option<&str>, right: Option<&str>) -> bool {
    left == right
}

fn ensure_owner_matches(expected: Option<&str>, actual: Option<&str>) -> Result<()> {
    if owners_match(expected, actual) {
        Ok(())
    } else {
        Err(anyhow!(
            "workspace session does not belong to the current account"
        ))
    }
}

fn workspace_id_from_file_name(name: &std::ffi::OsStr) -> Option<uuid::Uuid> {
    let name = name.to_str()?.strip_suffix(".json")?;
    uuid::Uuid::parse_str(name).ok()
}

fn write_private_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    if bytes.len() as u64 > MAX_WORKSPACE_FILE_BYTES {
        return Err(anyhow!("workspace metadata exceeds the file size limit"));
    }
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent", path.display()))?;
    cue_core::app_paths::create_private_dir(parent)?;
    let temporary = temporary_path(path);
    let result = (|| {
        let mut file = create_private_file(&temporary)?;
        file.write_all(&bytes)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        set_private_file_permissions(&temporary)?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temporary.display()))?;
        atomic_replace_file(&temporary, path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
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
        return Err(anyhow!("{} is not a regular file", path.display()));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(anyhow!("{} is a reparse-point file", path.display()));
        }
    }
    if metadata.len() > MAX_WORKSPACE_FILE_BYTES {
        return Err(anyhow!("{} exceeds the file size limit", path.display()));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::io::Read::by_ref(&mut file)
        .take(MAX_WORKSPACE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_WORKSPACE_FILE_BYTES {
        return Err(anyhow!("{} exceeds the file size limit", path.display()));
    }
    Ok(Some(bytes))
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

fn temporary_path(path: &Path) -> PathBuf {
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(
            "{TEMP_FILE_PREFIX}{}-{}",
            cue_core::clock::now_epoch_ms_string(),
            uuid::Uuid::new_v4().simple()
        ))
}

#[cfg(unix)]
fn atomic_replace_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
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

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)
        .with_context(|| format!("failed to open {} for sync", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync {}", path.display()))
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
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_core::{AssistantMode, WorkspaceInstructionsPatch, WorkspaceUpdateRequest};

    fn test_store(label: &str) -> (PathBuf, AppPaths, WorkspaceStore) {
        let base = std::env::temp_dir().join(format!(
            "bluey-workspace-store-{label}-{}",
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
        let store = WorkspaceStore::new(&paths).expect("workspace store");
        (base, paths, store)
    }

    fn create_request(title: &str) -> WorkspaceCreateRequest {
        WorkspaceCreateRequest {
            title: title.to_string(),
            profile: None,
            instructions: None,
        }
    }

    #[test]
    fn migration_is_stable_and_preserves_the_active_profile_once() {
        let (base, _paths, store) = test_store("migration");
        let mut meeting = MeetingRecord::new(Some("Existing session".to_string()));
        meeting.assistant_profile.mode = AssistantMode::SystemDesign;
        meeting.answer_instructions = Some("Lead with tradeoffs.".to_string());

        let first = store
            .migrate_default(None, Some(&meeting))
            .expect("first migration");
        let second = store
            .migrate_default(None, Some(&meeting))
            .expect("repeat migration");

        assert_eq!(first.id, second.id);
        assert_eq!(first.profile.mode, AssistantMode::SystemDesign);
        assert_eq!(first.instructions.as_deref(), Some("Lead with tradeoffs."));
        assert_eq!(store.list_for_owner(None).expect("list").len(), 1);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn owners_cannot_read_or_mutate_each_others_workspaces() {
        let (base, _paths, store) = test_store("owners");
        let workspace = store
            .create(Some("account-a"), create_request("A"))
            .expect("create A");

        assert!(store
            .get_for_owner(Some("account-b"), workspace.id)
            .is_err());
        assert!(store.activate(Some("account-b"), workspace.id).is_err());
        assert!(store
            .delete(Some("account-b"), workspace.id, workspace.revision)
            .is_err());
        assert!(store
            .list_for_owner(Some("account-b"))
            .expect("list B")
            .is_empty());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn stale_update_is_rejected_without_changing_the_record() {
        let (base, _paths, store) = test_store("stale-update");
        let workspace = store
            .create(None, create_request("Original"))
            .expect("create");
        let updated = store
            .update(
                None,
                WorkspaceUpdateRequest {
                    workspace_id: workspace.id,
                    expected_revision: workspace.revision,
                    title: Some("Current".to_string()),
                    profile: None,
                    instructions: WorkspaceInstructionsPatch::Unchanged,
                },
            )
            .expect("fresh update");
        assert!(store
            .update(
                None,
                WorkspaceUpdateRequest {
                    workspace_id: workspace.id,
                    expected_revision: workspace.revision,
                    title: Some("Stale".to_string()),
                    profile: None,
                    instructions: WorkspaceInstructionsPatch::Clear,
                },
            )
            .is_err());
        assert_eq!(
            store
                .get_for_owner(None, workspace.id)
                .expect("get")
                .expect("workspace")
                .title,
            "Current"
        );
        assert_eq!(updated.revision, workspace.revision + 1);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn stale_delete_cannot_hide_a_newer_workspace_revision() {
        let (base, _paths, store) = test_store("stale-delete");
        let workspace = store
            .create(None, create_request("Original"))
            .expect("create");
        let updated = store
            .update(
                None,
                WorkspaceUpdateRequest {
                    workspace_id: workspace.id,
                    expected_revision: workspace.revision,
                    title: Some("Newer".to_string()),
                    profile: None,
                    instructions: WorkspaceInstructionsPatch::Unchanged,
                },
            )
            .expect("update");

        assert!(store
            .delete(None, workspace.id, workspace.revision)
            .is_err());
        assert_eq!(
            store
                .get_for_owner(None, workspace.id)
                .expect("get")
                .expect("workspace")
                .revision,
            updated.revision
        );
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn ordinary_create_and_update_cannot_forge_jobs_provenance() {
        let (base, _paths, store) = test_store("jobs-provenance");
        let source = cue_core::AssistantSourceReference {
            application_id: Some("application-1".to_string()),
            receipt_id: Some("receipt-1".to_string()),
            resume_version_id: Some("resume-1".to_string()),
            receipt_fingerprint: Some("a".repeat(64)),
        };
        let forged_profile = AssistantProfile {
            source: Some(source),
            ..AssistantProfile::default()
        };
        assert!(store
            .create(
                None,
                WorkspaceCreateRequest {
                    title: "Forged".to_string(),
                    profile: Some(forged_profile.clone()),
                    instructions: None,
                },
            )
            .is_err());

        let workspace = store
            .create(None, create_request("Legitimate"))
            .expect("create legitimate workspace");
        assert!(store
            .update(
                None,
                WorkspaceUpdateRequest {
                    workspace_id: workspace.id,
                    expected_revision: workspace.revision,
                    title: None,
                    profile: Some(forged_profile),
                    instructions: WorkspaceInstructionsPatch::Unchanged,
                },
            )
            .is_err());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn syncing_an_existing_meeting_does_not_eagerly_create_another_default() {
        let (base, _paths, store) = test_store("sync-no-duplicate");
        let workspace = store.migrate_default(None, None).expect("default");
        let mut meeting = MeetingRecord::new(Some("Bound session".to_string()));
        meeting.workspace_id = Some(workspace.id);

        let synced = store
            .sync_active_settings_from_meeting(None, &meeting)
            .expect("sync meeting");
        assert_eq!(synced.id, workspace.id);
        assert_eq!(store.list_for_owner(None).expect("list").len(), 1);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn ordinary_session_sync_preserves_workspace_linked_job_metadata() {
        let (base, _paths, store) = test_store("sync-preserves-linked-job");
        let source = cue_core::AssistantSourceReference {
            application_id: Some("application-1".to_string()),
            receipt_id: Some("receipt-1".to_string()),
            resume_version_id: Some("resume-1".to_string()),
            receipt_fingerprint: Some("a".repeat(64)),
        };
        let profile = AssistantProfile {
            source: Some(source.clone()),
            ..AssistantProfile::default()
        };
        let mut workspace = WorkspaceRecord::new(
            Some("account-1".to_string()),
            "Linked job".to_string(),
            profile.clone(),
            None,
        )
        .expect("workspace");
        workspace.linked_job = Some(WorkspaceLinkedJobMetadata {
            import_id: "import-1".to_string(),
            context_sha256: "b".repeat(64),
            source,
            linked_at: cue_core::clock::now_epoch_ms_string(),
        });
        store
            .write_workspace_unlocked(&workspace)
            .expect("write linked workspace");
        store
            .write_active_pointer_unlocked(Some("account-1"), workspace.id)
            .expect("activate linked workspace");

        let mut meeting = MeetingRecord::new(Some("New linked session".to_string()));
        meeting.owner_account_id = Some("account-1".to_string());
        meeting.workspace_id = Some(workspace.id);
        meeting.assistant_profile = profile;
        let synced = store
            .sync_active_settings_from_meeting(Some("account-1"), &meeting)
            .expect("sync ordinary session");

        assert_eq!(
            synced.linked_job.as_ref().map(|job| job.import_id.as_str()),
            Some("import-1")
        );
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn activation_and_delete_are_owner_scoped_and_idempotent() {
        let (base, paths, store) = test_store("activation-delete");
        let first = store.migrate_default(None, None).expect("default");
        let second = store
            .create(None, create_request("Second"))
            .expect("second");
        store.activate(None, second.id).expect("activate second");
        assert_eq!(
            store.active_for_owner(None).expect("active").unwrap().id,
            second.id
        );

        let meeting_path = paths.data_dir.join("active-meeting.json");
        fs::write(&meeting_path, b"meeting bytes must survive").expect("dummy meeting");
        let deleted = store
            .delete(None, second.id, second.revision)
            .expect("delete active");
        assert!(deleted.deleted);
        assert_eq!(deleted.active_workspace_id, first.id);
        assert_eq!(
            fs::read(&meeting_path).expect("meeting survives"),
            b"meeting bytes must survive"
        );
        let repeated = store
            .delete(None, second.id, second.revision)
            .expect("repeat delete");
        assert!(!repeated.deleted);
        assert_eq!(repeated.active_workspace_id, first.id);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn delete_with_a_missing_pointer_never_selects_the_deleted_workspace_as_fallback() {
        let (base, _paths, store) = test_store("delete-missing-pointer");
        let first = store.migrate_default(None, None).expect("default");
        let second = store
            .create(None, create_request("Newer target"))
            .expect("second");
        fs::remove_file(&store.active_pointer_file).expect("remove pointer");

        let deleted = store
            .delete(None, second.id, second.revision)
            .expect("delete target");
        assert!(deleted.deleted);
        assert_eq!(deleted.active_workspace_id, first.id);
        assert_eq!(
            store.active_for_owner(None).expect("active").unwrap().id,
            first.id
        );
        assert!(store.get_for_owner(None, second.id).expect("get").is_none());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uncommitted_temp_never_replaces_a_workspace_or_pointer() {
        let (base, _paths, store) = test_store("atomic-crash");
        let workspace = store.migrate_default(None, None).expect("default");
        let workspace_temp = temporary_path(&store.workspace_path(workspace.id));
        fs::write(&workspace_temp, b"{uncommitted").expect("workspace temp");
        let pointer_temp = temporary_path(&store.active_pointer_file);
        fs::write(&pointer_temp, b"{uncommitted").expect("pointer temp");

        assert_eq!(
            store
                .get_for_owner(None, workspace.id)
                .expect("get")
                .expect("workspace")
                .id,
            workspace.id
        );
        assert_eq!(
            store.active_for_owner(None).expect("active").unwrap().id,
            workspace.id
        );
        assert!(workspace_temp.exists());
        assert!(pointer_temp.exists());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn corrupt_foreign_record_and_pointer_do_not_block_valid_owner_recovery() {
        let (base, _paths, store) = test_store("corrupt-isolation");
        let valid = store
            .create(Some("account-a"), create_request("Valid"))
            .expect("valid workspace");
        let corrupt_id = uuid::Uuid::new_v4();
        fs::write(store.workspace_path(corrupt_id), b"{corrupt").expect("corrupt record");
        fs::write(&store.active_pointer_file, b"{corrupt").expect("corrupt pointer");

        let recovered = store
            .migrate_default(Some("account-a"), None)
            .expect("recover pointer");
        assert_eq!(recovered.id, valid.id);
        assert_eq!(
            store
                .active_for_owner(Some("account-a"))
                .expect("active")
                .unwrap()
                .id,
            valid.id
        );
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn bounded_tombstone_compaction_preserves_meeting_files_and_future_creates() {
        let (base, paths, store) = test_store("tombstone-compaction");
        let meeting_path = paths.data_dir.join("meeting-must-survive.json");
        fs::write(&meeting_path, b"historic meeting").expect("meeting fixture");
        let mut other_owner = WorkspaceRecord::new(
            Some("account-other".to_string()),
            "Other owner tombstone".to_string(),
            AssistantProfile::default(),
            None,
        )
        .expect("other owner workspace");
        other_owner.deletion_state = WorkspaceDeletionState::Deleted {
            deleted_at: cue_core::clock::now_epoch_ms_string(),
        };
        store
            .write_workspace_unlocked(&other_owner)
            .expect("write other owner tombstone");
        for _ in 0..(MAX_RETAINED_DELETED_WORKSPACES + 4) {
            let mut workspace = WorkspaceRecord::new(
                Some("account-old".to_string()),
                "Deleted".to_string(),
                AssistantProfile::default(),
                None,
            )
            .expect("workspace");
            workspace.deletion_state = WorkspaceDeletionState::Deleted {
                deleted_at: cue_core::clock::now_epoch_ms_string(),
            };
            workspace.validate().expect("deleted workspace");
            store
                .write_workspace_unlocked(&workspace)
                .expect("write tombstone");
        }

        let current = store
            .create(Some("account-old"), create_request("Current"))
            .expect("create after compaction");
        assert!(store
            .get_for_owner(Some("account-old"), current.id)
            .expect("get current")
            .is_some());
        assert!(store.workspace_path(other_owner.id).exists());
        assert_eq!(
            fs::read(&meeting_path).expect("meeting survives"),
            b"historic meeting"
        );
        let retained_deleted = store
            .list_for_owner_unlocked(Some("account-old"), true)
            .expect("list retained tombstones")
            .into_iter()
            .filter(|workspace| !workspace.is_active())
            .count();
        assert!(retained_deleted <= MAX_RETAINED_DELETED_WORKSPACES);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn cloned_store_serializes_pointer_replacement_without_a_missing_window() {
        use std::sync::Barrier;
        use std::thread;

        let (base, _paths, store) = test_store("pointer-concurrency");
        let first = store.migrate_default(None, None).expect("default");
        let second = store
            .create(None, create_request("Second"))
            .expect("second");
        let barrier = Arc::new(Barrier::new(3));
        let writer_store = store.clone();
        let writer_barrier = barrier.clone();
        let writer = thread::spawn(move || {
            writer_barrier.wait();
            for index in 0..200 {
                writer_store
                    .activate(None, if index % 2 == 0 { first.id } else { second.id })
                    .expect("activate");
            }
        });
        let reader_store = store.clone();
        let reader_barrier = barrier.clone();
        let reader = thread::spawn(move || {
            reader_barrier.wait();
            for _ in 0..400 {
                assert!(reader_store
                    .active_for_owner(None)
                    .expect("active read")
                    .is_some());
            }
        });
        barrier.wait();
        writer.join().expect("writer");
        reader.join().expect("reader");
        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn workspace_reads_refuse_symlinks_and_files_are_private() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let (base, _paths, store) = test_store("nofollow-private");
        let workspace = store.migrate_default(None, None).expect("default");
        let path = store.workspace_path(workspace.id);
        assert_eq!(
            fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&store.active_pointer_file)
                .expect("pointer metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        let external = base.join("external.json");
        fs::write(&external, fs::read(&path).expect("workspace bytes")).expect("external");
        fs::remove_file(&path).expect("remove workspace");
        symlink(&external, &path).expect("symlink workspace");
        assert!(store.get_for_owner(None, workspace.id).is_err());
        let _ = fs::remove_dir_all(base);
    }
}
