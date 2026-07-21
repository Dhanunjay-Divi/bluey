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
    db::jobs::{self, CareerProfile, ResumeSourceAsset},
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
    let id = Uuid::new_v4().to_string();
    let storage_key = storage.resume_source_key(&account.id, &id, &sha256, &file_type);
    let now = jobs::now_ms();
    let asset = ResumeSourceAsset {
        id,
        file_name,
        media_type,
        file_type: file_type.clone(),
        storage_key: storage_key.clone(),
        sha256,
        size_bytes: i64::try_from(bytes.len()).unwrap_or(i64::MAX),
        page_count: input.page_count,
        template_status: template_status(&file_type).to_string(),
        created_at_ms: now,
        updated_at_ms: now,
    };

    storage
        .put(&storage_key, Bytes::from(bytes), &asset.media_type)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "source resume upload failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Bluey could not store this resume right now. Try again shortly.".to_string(),
            )
        })?;

    let mut profile = match input.profile {
        Some(profile) => {
            super::jobs::validate_profile(&profile)?;
            profile
        }
        None => jobs::get_profile(&state.pool, &account.id, &account.email).map_err(internal)?,
    };
    profile.source_resume_name = asset.file_name.clone();
    profile.source_resume_asset_id = asset.id.clone();
    profile.source_resume_sha256 = asset.sha256.clone();
    profile.source_resume_media_type = asset.media_type.clone();
    profile.source_resume_template_status = asset.template_status.clone();

    let (previous, saved_profile) = match jobs::save_resume_source_asset(
        &state.pool,
        &account.id,
        &asset,
        &profile,
    ) {
        Ok(saved) => saved,
        Err(error) => {
            if let Err(cleanup_error) = storage.delete(&storage_key).await {
                tracing::warn!(error = %cleanup_error, "failed to clean up source resume upload");
            }
            return Err(internal(error));
        }
    };

    if let Some(previous) = previous.filter(|previous| previous.storage_key != storage_key) {
        if storage.key_belongs_to_account(&previous.storage_key, &account.id) {
            if let Err(error) = storage.delete(&previous.storage_key).await {
                tracing::warn!(error = %error, "failed to remove replaced source resume");
            }
        }
    }

    Ok(Json(UploadResumeSourceResponse {
        asset: ResumeSourceMetadata::from(&asset),
        profile: saved_profile,
    }))
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
