//! Account-scoped source resume storage and evidence-locked template export.

use std::{io::Cursor, path::Path};

use axum::{
    body::Body,
    extract::{Path as AxumPath, State},
    http::{header, Response, StatusCode},
    Extension, Json,
};
use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zip::ZipArchive;

use crate::{
    api::AppState,
    auth::AuthedAccount,
    db::{
        account_data,
        jobs::{self, CareerProfile, ResumeSourceAsset},
        object_uploads::{self, NewObjectUpload, ObjectKind, StorageScope, UploadControlError},
    },
    jobs_resume_template,
    object_storage::{sha256_hex, ObjectStorage},
};

type ApiError = (StatusCode, String);

const MAX_SOURCE_RESUME_BYTES: usize = 10 * 1024 * 1024;
const DOCX_MEDIA_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
const PDF_MEDIA_TYPE: &str = "application/pdf";
const TEXT_MEDIA_TYPE: &str = "text/plain";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadResumeSourceRequest {
    pub request_id: String,
    pub file_name: String,
    pub media_type: String,
    pub bytes_base64: String,
    #[serde(default)]
    pub page_count: Option<i64>,
    #[serde(default)]
    pub profile: Option<CareerProfile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResumeSourceMetadata {
    pub id: String,
    pub file_name: String,
    pub media_type: String,
    pub file_type: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub page_count: Option<i64>,
    pub template_status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UploadResumeSourceResponse {
    pub asset: ResumeSourceMetadata,
    pub profile: CareerProfile,
}

impl From<&ResumeSourceAsset> for ResumeSourceMetadata {
    fn from(asset: &ResumeSourceAsset) -> Self {
        Self {
            id: asset.id.clone(),
            file_name: asset.file_name.clone(),
            media_type: asset.media_type.clone(),
            file_type: asset.file_type.clone(),
            sha256: asset.sha256.clone(),
            size_bytes: asset.size_bytes,
            page_count: asset.page_count,
            template_status: asset.template_status.clone(),
            created_at_ms: asset.created_at_ms,
            updated_at_ms: asset.updated_at_ms,
        }
    }
}

pub async fn resume_source(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Option<ResumeSourceMetadata>>, ApiError> {
    jobs::get_resume_source_asset(&state.pool, &account.id)
        .map(|asset| Json(asset.as_ref().map(ResumeSourceMetadata::from)))
        .map_err(internal)
}

pub async fn upload_resume_source(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(input): Json<UploadResumeSourceRequest>,
) -> Result<Json<UploadResumeSourceResponse>, ApiError> {
    let request_id = Uuid::parse_str(input.request_id.trim()).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid resume upload request identifier.".to_string(),
        )
    })?;
    let (file_name, file_type, media_type) =
        validate_source_identity(&input.file_name, &input.media_type, input.page_count)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input.bytes_base64.trim())
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "The selected resume could not be decoded. Choose the file again.".to_string(),
            )
        })?;
    validate_source_bytes(&file_type, &bytes)?;

    let storage_config = state.config.object_storage.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Resume storage is temporarily unavailable. Try again shortly.".to_string(),
    ))?;
    let storage = ObjectStorage::new(storage_config);
    if bytes.len() > MAX_SOURCE_RESUME_BYTES || bytes.len() > storage.max_object_bytes() {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "Choose a resume smaller than 10 MB.".to_string(),
        ));
    }

    let sha256 = sha256_hex(&bytes);
    let id = request_id.to_string();
    let logical_id = format!("jobs-resume-source:{id}");
    let storage_key = storage.resume_source_key(&account.id, &id, &sha256, &file_type);
    let now = jobs::now_ms();
    let size_bytes = i64::try_from(bytes.len()).map_err(|_| {
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            "Choose a resume smaller than 10 MB.".to_string(),
        )
    })?;
    let _object_writer = account_data::acquire_account_object_writer(&state.pool, &account.id)
        .await
        .map_err(internal)?;
    // A request's predecessor is immutable lineage, not whatever resume is
    // current when a delayed retry finally reaches publication. Recover the
    // stored fence for an existing request; otherwise snapshot the current
    // pointer before reserving the new logical object.
    let existing_upload =
        object_uploads::artifact_upload(&state.pool, &account.id, &logical_id).map_err(internal)?;
    let (stored_profile, stored_profile_revision) =
        jobs::get_resume_upload_profile(&state.pool, &account.id, &account.email)
            .map_err(internal)?;
    let (
        profile,
        profile_mode,
        base_profile_sha256,
        requested_profile_sha256,
        replaces_source_asset_id,
    ) = if let Some(existing) = existing_upload.as_ref() {
        let authority = resume_upload_authority(existing).map_err(resume_upload_control_error)?;
        let profile = match (authority.profile_mode.as_str(), input.profile) {
            ("replace", Some(profile)) => {
                super::jobs::validate_profile(&profile)?;
                if requested_resume_profile_sha256(&profile).map_err(internal)?
                    != authority
                        .requested_profile_sha256
                        .as_deref()
                        .unwrap_or_default()
                {
                    return Err(resume_upload_control_error(
                        UploadControlError::IdempotencyConflict.into(),
                    ));
                }
                profile
            }
            ("merge_source", None) => stored_profile,
            _ => {
                return Err(resume_upload_control_error(
                    UploadControlError::IdempotencyConflict.into(),
                ))
            }
        };
        (
            profile,
            authority.profile_mode,
            authority.base_profile_sha256,
            authority.requested_profile_sha256,
            authority.replaces_source_asset_id,
        )
    } else {
        let replaces_source_asset_id = jobs::get_resume_source_asset(&state.pool, &account.id)
            .map_err(internal)?
            .map(|asset| asset.id);
        match input.profile {
            Some(profile) => {
                super::jobs::validate_profile(&profile)?;
                if profile.updated_at_ms != stored_profile.updated_at_ms {
                    return Err(resume_upload_control_error(
                        UploadControlError::IdempotencyConflict.into(),
                    ));
                }
                let requested_profile_sha256 =
                    requested_resume_profile_sha256(&profile).map_err(internal)?;
                (
                    profile,
                    "replace".to_string(),
                    stored_profile_revision,
                    Some(requested_profile_sha256),
                    replaces_source_asset_id,
                )
            }
            None => (
                stored_profile,
                "merge_source".to_string(),
                None,
                None,
                replaces_source_asset_id,
            ),
        }
    };
    let reservation = object_uploads::reserve_account_object_upload(
        &state.pool,
        &NewObjectUpload {
            account_id: account.id.clone(),
            object_kind: ObjectKind::Artifact,
            logical_id,
            session_id: None,
            storage_scope: StorageScope::Artifact,
            object_key: storage_key.clone(),
            size_bytes,
            sha256: sha256.clone(),
            content_type: media_type.clone(),
            expires_at_ms: i64::MAX,
            metadata_json: serde_json::json!({
                "artifact_class": "jobs_resume_source",
                "jobs_resume_source_asset_id": id,
                "request_id": request_id,
                "profile_mode": profile_mode,
                "base_profile_sha256": base_profile_sha256,
                "requested_profile_sha256": requested_profile_sha256,
                "replaces_source_asset_id": replaces_source_asset_id,
                "file_name": file_name,
                "file_type": file_type,
                "media_type": media_type,
                "page_count": input.page_count,
                "retention_policy": "account_lifetime_until_deletion",
            }),
            now_ms: now,
            limits: storage.upload_limits(),
        },
    )
    .map_err(resume_upload_control_error)?;
    let asset = ResumeSourceAsset {
        id,
        file_name,
        media_type: reservation.upload.content_type.clone(),
        file_type: file_type.clone(),
        storage_key: reservation.upload.object_key.clone(),
        sha256: reservation.upload.sha256.clone(),
        size_bytes: reservation.upload.size_bytes,
        page_count: input.page_count,
        template_status: template_status(&file_type).to_string(),
        created_at_ms: reservation.upload.created_at_ms,
        updated_at_ms: reservation.upload.created_at_ms,
    };
    let upload_bytes = Bytes::from(bytes);
    if reservation.needs_put {
        object_uploads::begin_upload_put(&state.pool, &reservation.upload.id, jobs::now_ms())
            .map_err(resume_upload_control_error)?;
        if let Err(error) = storage
            .put(&asset.storage_key, upload_bytes.clone(), &asset.media_type)
            .await
        {
            let _ = object_uploads::record_put_failure(
                &state.pool,
                &reservation.upload.id,
                &error.to_string(),
                jobs::now_ms(),
            );
            tracing::warn!(error = %error, "source resume upload failed");
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "Bluey could not store this resume right now. Try again shortly.".to_string(),
            ));
        }
    }
    let stored = storage.get(&asset.storage_key).await.map_err(|error| {
        let _ = object_uploads::record_put_failure(
            &state.pool,
            &reservation.upload.id,
            &error.to_string(),
            jobs::now_ms(),
        );
        tracing::warn!(error = %error, "source resume read-back failed");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey could not verify this resume right now. Try again shortly.".to_string(),
        )
    })?;
    if stored.bytes != upload_bytes
        || sha256_hex(&stored.bytes) != asset.sha256
        || !stored
            .content_type
            .split(';')
            .next()
            .is_some_and(|value| value.eq_ignore_ascii_case(&asset.media_type))
    {
        let _ = object_uploads::record_put_failure(
            &state.pool,
            &reservation.upload.id,
            "source resume read-back verification failed",
            jobs::now_ms(),
        );
        return Err((
            StatusCode::BAD_GATEWAY,
            "Stored resume failed integrity verification. Try again shortly.".to_string(),
        ));
    }
    if reservation.needs_put {
        object_uploads::release_verified_upload_put(
            &state.pool,
            &reservation.upload.id,
            jobs::now_ms(),
        )
        .map_err(resume_upload_control_error)?;
    }
    let publication = match jobs::publish_resume_source_asset(
        &state.pool,
        &account.id,
        &asset,
        &profile,
        &reservation.upload.id,
    ) {
        Ok(saved) => saved,
        Err(error) => {
            // Publication, the profile pointer, and replacement cleanup share
            // one database transaction. On a definite or uncertain commit
            // error, leave the verified object pending: an exact retry can
            // reconcile it, and the stale-upload worker can delete it later.
            return Err(resume_upload_control_error(error));
        }
    };

    Ok(Json(UploadResumeSourceResponse {
        asset: ResumeSourceMetadata::from(&publication.asset),
        profile: publication.profile,
    }))
}

struct ResumeUploadRequestAuthority {
    profile_mode: String,
    base_profile_sha256: Option<String>,
    requested_profile_sha256: Option<String>,
    replaces_source_asset_id: Option<String>,
}

fn resume_upload_authority(
    upload: &object_uploads::ObjectUpload,
) -> anyhow::Result<ResumeUploadRequestAuthority> {
    let metadata = serde_json::from_str::<serde_json::Value>(&upload.metadata_json)
        .map_err(|_| UploadControlError::IdempotencyConflict)?;
    let profile_mode = metadata
        .get("profile_mode")
        .and_then(serde_json::Value::as_str)
        .filter(|value| matches!(*value, "replace" | "merge_source"))
        .ok_or(UploadControlError::IdempotencyConflict)?
        .to_string();
    let base_profile_sha256 = optional_resume_upload_hash(&metadata, "base_profile_sha256")?;
    let requested_profile_sha256 =
        optional_resume_upload_hash(&metadata, "requested_profile_sha256")?;
    if (profile_mode == "replace" && requested_profile_sha256.is_none())
        || (profile_mode == "merge_source"
            && (base_profile_sha256.is_some() || requested_profile_sha256.is_some()))
    {
        return Err(UploadControlError::IdempotencyConflict.into());
    }
    let replaces_source_asset_id = match metadata.get("replaces_source_asset_id") {
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => return Err(UploadControlError::IdempotencyConflict.into()),
    };
    Ok(ResumeUploadRequestAuthority {
        profile_mode,
        base_profile_sha256,
        requested_profile_sha256,
        replaces_source_asset_id,
    })
}

fn optional_resume_upload_hash(
    metadata: &serde_json::Value,
    field: &'static str,
) -> anyhow::Result<Option<String>> {
    match metadata.get(field) {
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value))
            if value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) =>
        {
            Ok(Some(value.clone()))
        }
        _ => Err(UploadControlError::IdempotencyConflict.into()),
    }
}

fn requested_resume_profile_sha256(profile: &CareerProfile) -> anyhow::Result<String> {
    Ok(sha256_hex(&serde_json::to_vec(profile)?))
}

pub async fn download_template_docx(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    AxumPath(resume_version_id): AxumPath<String>,
) -> Result<Response<Body>, ApiError> {
    let asset = jobs::get_resume_source_asset(&state.pool, &account.id)
        .map_err(internal)?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Upload the original DOCX resume before downloading this template.".to_string(),
        ))?;
    if asset.template_status != "exact_docx" || asset.file_type != "docx" {
        return Err((
            StatusCode::CONFLICT,
            "Exact template export needs the original DOCX. PDF resumes use Bluey's ATS layout."
                .to_string(),
        ));
    }
    let resume = jobs::get_resume_version(&state.pool, &account.id, &resume_version_id)
        .map_err(internal)?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Resume version not found.".to_string(),
        ))?;
    let storage_config = state.config.object_storage.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Resume storage is temporarily unavailable. Try again shortly.".to_string(),
    ))?;
    let storage = ObjectStorage::new(storage_config);
    if !storage.key_belongs_to_account(&asset.storage_key, &account.id) {
        return Err((
            StatusCode::NOT_FOUND,
            "Resume source not found.".to_string(),
        ));
    }
    let stored = storage.get(&asset.storage_key).await.map_err(|error| {
        tracing::warn!(error = %error, "source resume download failed");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey could not load the original resume right now.".to_string(),
        )
    })?;
    if sha256_hex(&stored.bytes) != asset.sha256 {
        tracing::error!(asset_id = %asset.id, "source resume checksum mismatch");
        return Err((
            StatusCode::CONFLICT,
            "The original resume did not pass integrity verification.".to_string(),
        ));
    }
    let tailored = jobs_resume_template::patch_docx(&stored.bytes, &resume.diff).map_err(|error| {
        tracing::warn!(error = %error, "exact template resume patch failed closed");
        (
            StatusCode::CONFLICT,
            "Bluey could not safely apply this version to the original template. Download the ATS version instead."
                .to_string(),
        )
    })?;
    let file_name = download_file_name(&asset.file_name, resume.version_no);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, DOCX_MEDIA_TYPE)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{file_name}\""),
        )
        .header("x-content-type-options", "nosniff")
        .body(Body::from(tailored))
        .map_err(|error| internal(error.into()))
}

fn validate_source_identity(
    raw_file_name: &str,
    raw_media_type: &str,
    page_count: Option<i64>,
) -> Result<(String, String, String), ApiError> {
    let file_name = raw_file_name.trim();
    if file_name.is_empty() || file_name.len() > 180 {
        return bad_request("Choose a resume with a valid file name.");
    }
    if page_count.is_some_and(|count| !(1..=20).contains(&count)) {
        return bad_request("PDF resumes can contain up to 20 pages.");
    }
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (expected_media_type, file_type) = match extension.as_str() {
        "docx" => (DOCX_MEDIA_TYPE, "docx"),
        "pdf" => (PDF_MEDIA_TYPE, "pdf"),
        "txt" => (TEXT_MEDIA_TYPE, "txt"),
        _ => return bad_request("Choose a PDF, DOCX, or TXT resume."),
    };
    let media_type = raw_media_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if media_type != expected_media_type {
        return bad_request("The resume file type does not match its contents.");
    }
    Ok((
        file_name.to_string(),
        file_type.to_string(),
        expected_media_type.to_string(),
    ))
}

fn validate_source_bytes(file_type: &str, bytes: &[u8]) -> Result<(), ApiError> {
    if bytes.is_empty() {
        return bad_request("The selected resume is empty.");
    }
    match file_type {
        "docx" => {
            let mut archive = ZipArchive::new(Cursor::new(bytes))
                .map_err(|_| bad_request_error("The selected DOCX could not be opened."))?;
            for required in ["[Content_Types].xml", "word/document.xml"] {
                if archive.by_name(required).is_err() {
                    return bad_request("The selected file is not a valid DOCX resume.");
                }
            }
        }
        "pdf" if !bytes.starts_with(b"%PDF-") => {
            return bad_request("The selected file is not a valid PDF resume.");
        }
        "txt" if std::str::from_utf8(bytes).is_err() => {
            return bad_request("The selected text resume must use UTF-8.");
        }
        "pdf" | "txt" => {}
        _ => return bad_request("Choose a PDF, DOCX, or TXT resume."),
    }
    Ok(())
}

fn template_status(file_type: &str) -> &'static str {
    match file_type {
        "docx" => "exact_docx",
        "pdf" => "converted_layout",
        _ => "text_only",
    }
}

fn download_file_name(source_name: &str, version_no: i64) -> String {
    let stem = Path::new(source_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("resume")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{stem}-bluey-v{}.docx", version_no.max(1))
}

fn bad_request<T>(message: &str) -> Result<T, ApiError> {
    Err(bad_request_error(message))
}

fn bad_request_error(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, message.to_string())
}

fn internal(error: anyhow::Error) -> ApiError {
    tracing::error!(error = %error, "Jobs resume source request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Bluey could not complete that resume request.".to_string(),
    )
}

fn resume_upload_control_error(error: anyhow::Error) -> ApiError {
    match error.downcast_ref::<UploadControlError>() {
        Some(UploadControlError::ObjectTooLarge) => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "Choose a resume smaller than 10 MB.".to_string(),
        ),
        Some(
            UploadControlError::AccountBytesQuotaExceeded
            | UploadControlError::AccountObjectQuotaExceeded,
        ) => (
            StatusCode::INSUFFICIENT_STORAGE,
            "Resume storage is full. Remove older account data and try again.".to_string(),
        ),
        Some(UploadControlError::DailyQuotaExceeded) => (
            StatusCode::TOO_MANY_REQUESTS,
            "Resume upload capacity is temporarily unavailable. Try again later.".to_string(),
        ),
        Some(UploadControlError::AccountDeleting) => (
            StatusCode::CONFLICT,
            "Account deletion has already fenced new resume uploads.".to_string(),
        ),
        Some(
            UploadControlError::IdempotencyConflict
            | UploadControlError::UploadInProgress
            | UploadControlError::UploadGone,
        ) => (
            StatusCode::CONFLICT,
            "This resume upload can no longer be completed. Choose the file again.".to_string(),
        ),
        _ => internal(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn valid_docx() -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("[Content_Types].xml", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"<Types/>").unwrap();
        writer
            .start_file("word/document.xml", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"<w:document/>").unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn validates_supported_source_types() {
        assert!(validate_source_bytes("docx", &valid_docx()).is_ok());
        assert!(validate_source_bytes("pdf", b"%PDF-1.7\n").is_ok());
        assert!(validate_source_bytes("txt", b"Resume").is_ok());
    }

    #[test]
    fn rejects_spoofed_source_types() {
        assert!(validate_source_bytes("docx", b"not a zip").is_err());
        assert!(validate_source_bytes("pdf", b"not a pdf").is_err());
        assert!(validate_source_bytes("txt", &[0xff]).is_err());
    }

    #[test]
    fn download_name_is_safe_and_versioned() {
        assert_eq!(
            download_file_name("Taylor Rivera Resume.docx", 3),
            "Taylor_Rivera_Resume-bluey-v3.docx"
        );
    }
}
