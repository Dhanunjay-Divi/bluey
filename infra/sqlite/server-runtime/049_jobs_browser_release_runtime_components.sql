-- Exact runtime components measured from the sealed Browser package and bound by
-- the signed release-manifest artifact. These rows are immutable projections;
-- the canonical signed manifest remains the authority.
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS jobs_browser_release_artifact_runtime_components (
  manifest_sha256             TEXT NOT NULL
    CHECK(length(manifest_sha256) = 64 AND lower(manifest_sha256) = manifest_sha256),
  artifact_id                 TEXT NOT NULL CHECK(length(artifact_id) > 0),
  build_descriptor_sha256     TEXT NOT NULL
    CHECK(length(build_descriptor_sha256) = 64
      AND lower(build_descriptor_sha256) = build_descriptor_sha256),
  artifact_sha256             TEXT NOT NULL
    CHECK(length(artifact_sha256) = 64 AND lower(artifact_sha256) = artifact_sha256),
  platform                    TEXT NOT NULL CHECK(platform IN ('darwin', 'windows')),
  architecture                TEXT NOT NULL CHECK(architecture IN ('arm64', 'x64')),
  package_kind                TEXT NOT NULL
    CHECK(package_kind IN ('darwin-dmg', 'darwin-zip', 'windows-nsis')),
  automation_bundle_sha256    TEXT NOT NULL
    CHECK(length(automation_bundle_sha256) = 64
      AND lower(automation_bundle_sha256) = automation_bundle_sha256),
  chromium_executable_sha256  TEXT NOT NULL
    CHECK(length(chromium_executable_sha256) = 64
      AND lower(chromium_executable_sha256) = chromium_executable_sha256),
  recorded_at_ms              INTEGER NOT NULL CHECK(recorded_at_ms >= 0),
  PRIMARY KEY(manifest_sha256, artifact_id),
  UNIQUE(manifest_sha256, platform, architecture, package_kind),
  FOREIGN KEY(
    manifest_sha256, artifact_id, build_descriptor_sha256, artifact_sha256,
    platform, architecture, package_kind
  ) REFERENCES jobs_browser_release_artifacts(
    manifest_sha256, artifact_id, build_descriptor_sha256, artifact_sha256,
    platform, architecture, package_kind
  ) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_runtime_components_target
  ON jobs_browser_release_artifact_runtime_components(
    manifest_sha256, platform, architecture, package_kind,
    build_descriptor_sha256
  );

CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_runtime_components_no_update
BEFORE UPDATE ON jobs_browser_release_artifact_runtime_components
BEGIN
  SELECT RAISE(ABORT, 'Browser release runtime components are append-only');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_runtime_components_no_delete
BEFORE DELETE ON jobs_browser_release_artifact_runtime_components
BEGIN
  SELECT RAISE(ABORT, 'Browser release runtime components are append-only');
END;
