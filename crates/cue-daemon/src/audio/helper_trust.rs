#[cfg(windows)]
use std::io::Read;
#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use sha2::{Digest, Sha256};

#[cfg(any(windows, test))]
#[derive(serde::Deserialize)]
struct IntegrityManifest {
    schema_version: u8,
    product: String,
    files: Vec<IntegrityEntry>,
}

#[cfg(any(windows, test))]
#[derive(serde::Deserialize)]
struct IntegrityEntry {
    path: String,
    sha256: String,
    size_bytes: u64,
}

#[cfg(any(windows, test))]
fn parse_integrity_manifest(bytes: &[u8]) -> Option<IntegrityManifest> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return None;
    }
    let manifest = serde_json::from_slice::<IntegrityManifest>(bytes).ok()?;
    (manifest.schema_version == 1 && manifest.product == "Bluey" && manifest.files.len() <= 64)
        .then_some(manifest)
}

#[cfg(windows)]
pub(crate) fn packaged_windows_helper_integrity_matches(helper: &Path) -> bool {
    const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

    let Some(parent) = helper.parent() else {
        return false;
    };
    let manifest_path = parent.join("bluey-integrity.json");
    match std::fs::symlink_metadata(&manifest_path) {
        Ok(metadata)
            if metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() <= MAX_MANIFEST_BYTES => {}
        _ => return cfg!(debug_assertions),
    }
    let bytes = match std::fs::read(&manifest_path) {
        Ok(bytes) if bytes.len() as u64 <= MAX_MANIFEST_BYTES => bytes,
        _ => return false,
    };
    let manifest = match parse_integrity_manifest(&bytes) {
        Some(manifest) => manifest,
        None => return false,
    };
    let Some(file_name) = helper.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let mut matches = manifest.files.iter().filter(|entry| {
        !entry.path.contains('/')
            && !entry.path.contains('\\')
            && entry.path.eq_ignore_ascii_case(file_name)
    });
    let Some(expected) = matches.next() else {
        return false;
    };
    if matches.next().is_some()
        || expected.sha256.len() != 64
        || !expected.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return false;
    }
    let helper_metadata = match helper.metadata() {
        Ok(metadata) => metadata,
        Err(_) => return false,
    };
    if helper_metadata.len() != expected.size_bytes {
        return false;
    }
    let mut file = match std::fs::File::open(helper) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => digest.update(&buffer[..read]),
            Err(_) => return false,
        }
    }
    hex::encode(digest.finalize()).eq_ignore_ascii_case(&expected.sha256)
}

#[cfg(test)]
mod tests {
    use super::parse_integrity_manifest;

    const VALID: &[u8] = br#"{"schema_version":1,"product":"Bluey","build_id":"test","files":[{"path":"audio-driver.exe","sha256":"0000000000000000000000000000000000000000000000000000000000000000","size_bytes":1}]}"#;

    #[test]
    fn package_manifest_round_trips_without_bom() {
        let manifest = parse_integrity_manifest(VALID).expect("valid manifest");
        assert_eq!(manifest.product, "Bluey");
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].path, "audio-driver.exe");
        assert_eq!(manifest.files[0].sha256.len(), 64);
        assert_eq!(manifest.files[0].size_bytes, 1);
    }

    #[test]
    fn package_manifest_rejects_bom_and_wrong_product() {
        let with_bom = [b"\xef\xbb\xbf".as_slice(), VALID].concat();
        assert!(parse_integrity_manifest(&with_bom).is_none());
        let wrong = String::from_utf8(VALID.to_vec())
            .expect("utf8")
            .replace("\"Bluey\"", "\"Other\"");
        assert!(parse_integrity_manifest(wrong.as_bytes()).is_none());
    }
}
