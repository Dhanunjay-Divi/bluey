-- Target: SQLite
-- Adopt pre-ledger Jobs account objects into the durable object lifecycle.

INSERT INTO object_uploads (
  id, account_id, object_kind, logical_id, session_id, storage_scope,
  object_key, size_bytes, sha256, content_type, expires_at_ms, state,
  metadata_json, created_at_ms, updated_at_ms, uploaded_at_ms, deleted_at_ms
)
SELECT
  'backfill:jobs-resume-source:' || id,
  account_id,
  'artifact',
  'jobs-resume-source:' || id,
  NULL,
  'artifact',
  storage_key,
  size_bytes,
  lower(sha256),
  media_type,
  9223372036854775807,
  'ready',
  json_object(
    'artifact_class', 'jobs_resume_source',
    'jobs_resume_source_asset_id', id,
    'file_name', file_name,
    'file_type', file_type,
    'media_type', media_type,
    'page_count', page_count,
    'retention_policy', 'account_lifetime_until_deletion'
  ),
  created_at_ms,
  updated_at_ms,
  updated_at_ms,
  NULL
FROM jobs_resume_source_assets
WHERE TRUE
ON CONFLICT (account_id, object_kind, logical_id) DO NOTHING;

INSERT INTO object_uploads (
  id, account_id, object_kind, logical_id, session_id, storage_scope,
  object_key, size_bytes, sha256, content_type, expires_at_ms, state,
  metadata_json, created_at_ms, updated_at_ms, uploaded_at_ms, deleted_at_ms
)
SELECT
  'backfill:jobs-browser-profile:' || browser_profile_id || ':' || generation,
  account_id,
  'artifact',
  'jobs-browser-profile:' || browser_profile_id || ':' || generation || ':' ||
    envelope_version || ':' || lower(sha256),
  NULL,
  'artifact',
  object_key,
  size_bytes,
  lower(sha256),
  'application/vnd.bluey.browser-profile+encrypted',
  9223372036854775807,
  'ready',
  json_object(
    'artifact_class', 'jobs_browser_profile_snapshot',
    'jobs_browser_profile_id', browser_profile_id,
    'generation', generation,
    'envelope_version', envelope_version,
    'retention_policy', 'account_lifetime_until_deletion'
  ),
  updated_at_ms,
  updated_at_ms,
  updated_at_ms,
  NULL
FROM jobs_browser_profile_snapshots
WHERE TRUE
ON CONFLICT (account_id, object_kind, logical_id) DO NOTHING;

-- A targeted logical-identity replay is safe only when every immutable field
-- still matches the canonical Jobs row. Use a TEMP CHECK as SQLite's portable
-- migration assertion primitive; inserting 0 aborts startup on drift instead
-- of silently accepting a conflicting object-key or payload binding.
DROP TABLE IF EXISTS temp.jobs_account_object_backfill_validation;
CREATE TEMP TABLE jobs_account_object_backfill_validation (
  valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO jobs_account_object_backfill_validation (valid)
SELECT CASE WHEN
  EXISTS (
    SELECT 1
      FROM jobs_resume_source_assets source
     WHERE NOT EXISTS (
       SELECT 1
         FROM object_uploads upload
        WHERE upload.account_id = source.account_id
          AND upload.object_kind = 'artifact'
          AND upload.logical_id = 'jobs-resume-source:' || source.id
          AND upload.session_id IS NULL
          AND upload.storage_scope = 'artifact'
          AND upload.object_key = source.storage_key
          AND upload.size_bytes = source.size_bytes
          AND lower(upload.sha256) = lower(source.sha256)
          AND upload.content_type = source.media_type
          AND upload.expires_at_ms = 9223372036854775807
          AND upload.state = 'ready'
          AND upload.uploaded_at_ms IS NOT NULL
          AND upload.deleted_at_ms IS NULL
          AND json_type(upload.metadata_json) = 'object'
          AND json_extract(upload.metadata_json, '$.artifact_class') =
                'jobs_resume_source'
          AND json_extract(upload.metadata_json, '$.jobs_resume_source_asset_id') = source.id
          AND json_extract(upload.metadata_json, '$.file_name') = source.file_name
          AND json_extract(upload.metadata_json, '$.file_type') = source.file_type
          AND json_extract(upload.metadata_json, '$.media_type') = source.media_type
          AND json_extract(upload.metadata_json, '$.page_count') IS source.page_count
          AND json_extract(upload.metadata_json, '$.retention_policy') =
                'account_lifetime_until_deletion'
          AND (
            (SELECT COUNT(*) FROM json_each(upload.metadata_json)) = 7
            OR (
              (SELECT COUNT(*) FROM json_each(upload.metadata_json)) = 12
              AND json_type(upload.metadata_json, '$.request_id') = 'text'
              AND json_extract(upload.metadata_json, '$.request_id') = source.id
              AND json_extract(upload.metadata_json, '$.profile_mode') IN (
                    'replace', 'merge_source'
                  )
              AND (
                json_type(upload.metadata_json, '$.base_profile_sha256') = 'null'
                OR (
                  json_type(upload.metadata_json, '$.base_profile_sha256') = 'text'
                  AND length(
                        json_extract(upload.metadata_json, '$.base_profile_sha256')
                      ) = 64
                  AND json_extract(upload.metadata_json, '$.base_profile_sha256')
                        NOT GLOB '*[^0-9a-f]*'
                )
              )
              AND NOT (
                json_extract(upload.metadata_json, '$.profile_mode') = 'merge_source'
                AND json_type(upload.metadata_json, '$.base_profile_sha256') <> 'null'
              )
              AND (
                json_type(upload.metadata_json, '$.requested_profile_sha256') = 'null'
                OR (
                  json_type(upload.metadata_json, '$.requested_profile_sha256') = 'text'
                  AND length(
                        json_extract(
                          upload.metadata_json,
                          '$.requested_profile_sha256'
                        )
                      ) = 64
                  AND json_extract(upload.metadata_json, '$.requested_profile_sha256')
                        NOT GLOB '*[^0-9a-f]*'
                )
              )
              AND (
                (
                  json_extract(upload.metadata_json, '$.profile_mode') = 'replace'
                  AND json_type(
                        upload.metadata_json,
                        '$.requested_profile_sha256'
                      ) = 'text'
                )
                OR (
                  json_extract(upload.metadata_json, '$.profile_mode') = 'merge_source'
                  AND json_type(
                        upload.metadata_json,
                        '$.requested_profile_sha256'
                      ) = 'null'
                )
              )
              AND (
                json_type(upload.metadata_json, '$.replaces_source_asset_id') = 'null'
                OR (
                  json_type(upload.metadata_json, '$.replaces_source_asset_id') = 'text'
                  AND length(
                        json_extract(
                          upload.metadata_json,
                          '$.replaces_source_asset_id'
                        )
                      ) > 0
                )
              )
            )
          )
     )
  )
  OR EXISTS (
    SELECT 1
      FROM jobs_browser_profile_snapshots snapshot
     WHERE NOT EXISTS (
       SELECT 1
         FROM object_uploads upload
        WHERE upload.account_id = snapshot.account_id
          AND upload.object_kind = 'artifact'
          AND upload.logical_id = 'jobs-browser-profile:' || snapshot.browser_profile_id || ':' ||
                snapshot.generation || ':' || snapshot.envelope_version || ':' ||
                lower(snapshot.sha256)
          AND upload.session_id IS NULL
          AND upload.storage_scope = 'artifact'
          AND upload.object_key = snapshot.object_key
          AND upload.size_bytes = snapshot.size_bytes
          AND lower(upload.sha256) = lower(snapshot.sha256)
          AND upload.content_type = 'application/vnd.bluey.browser-profile+encrypted'
          AND upload.expires_at_ms = 9223372036854775807
          AND upload.state = 'ready'
          AND upload.uploaded_at_ms IS NOT NULL
          AND upload.deleted_at_ms IS NULL
          AND json_type(upload.metadata_json) = 'object'
          AND json_extract(upload.metadata_json, '$.artifact_class') =
                'jobs_browser_profile_snapshot'
          AND json_extract(upload.metadata_json, '$.jobs_browser_profile_id') =
                snapshot.browser_profile_id
          AND json_extract(upload.metadata_json, '$.generation') = snapshot.generation
          AND json_extract(upload.metadata_json, '$.envelope_version') =
                snapshot.envelope_version
          AND json_extract(upload.metadata_json, '$.retention_policy') =
                'account_lifetime_until_deletion'
          AND (
            (SELECT COUNT(*) FROM json_each(upload.metadata_json)) = 5
            OR (
              (SELECT COUNT(*) FROM json_each(upload.metadata_json)) = 9
              AND json_type(upload.metadata_json, '$.jobs_application_id') = 'text'
              AND length(
                    json_extract(upload.metadata_json, '$.jobs_application_id')
                  ) > 0
              AND json_extract(upload.metadata_json, '$.jobs_run_id') =
                    snapshot.writer_run_id
              AND json_extract(upload.metadata_json, '$.writer_fence') =
                    snapshot.writer_fence
              AND json_extract(upload.metadata_json, '$.expected_generation') =
                    snapshot.generation - 1
            )
          )
     )
  )
THEN 0 ELSE 1 END;

INSERT INTO object_storage_outbox (
  id, upload_id, account_id, operation, state, attempt_count,
  next_attempt_at_ms, last_error, created_at_ms, updated_at_ms, completed_at_ms
)
SELECT
  upload.id || ':put', upload.id, upload.account_id, 'put', 'completed', 1,
  upload.uploaded_at_ms, NULL, upload.created_at_ms, upload.uploaded_at_ms,
  upload.uploaded_at_ms
FROM jobs_resume_source_assets source
JOIN object_uploads upload
  ON upload.account_id = source.account_id
 AND upload.object_kind = 'artifact'
 AND upload.logical_id = 'jobs-resume-source:' || source.id
WHERE TRUE
ON CONFLICT (upload_id, operation) DO NOTHING;

INSERT INTO object_storage_outbox (
  id, upload_id, account_id, operation, state, attempt_count,
  next_attempt_at_ms, last_error, created_at_ms, updated_at_ms, completed_at_ms
)
SELECT
  upload.id || ':put', upload.id, upload.account_id, 'put', 'completed', 1,
  upload.uploaded_at_ms, NULL, upload.created_at_ms, upload.uploaded_at_ms,
  upload.uploaded_at_ms
FROM jobs_browser_profile_snapshots snapshot
JOIN object_uploads upload
  ON upload.account_id = snapshot.account_id
 AND upload.object_kind = 'artifact'
 AND upload.logical_id = 'jobs-browser-profile:' || snapshot.browser_profile_id || ':' ||
       snapshot.generation || ':' || snapshot.envelope_version || ':' || lower(snapshot.sha256)
WHERE TRUE
ON CONFLICT (upload_id, operation) DO NOTHING;

DELETE FROM jobs_account_object_backfill_validation;
INSERT INTO jobs_account_object_backfill_validation (valid)
SELECT CASE WHEN EXISTS (
  SELECT 1
    FROM object_uploads upload
   WHERE upload.state = 'ready'
     AND upload.session_id IS NULL
     AND json_extract(upload.metadata_json, '$.artifact_class') IN (
           'jobs_resume_source', 'jobs_browser_profile_snapshot'
         )
     AND NOT EXISTS (
       SELECT 1
         FROM object_storage_outbox outbox
        WHERE outbox.id = upload.id || ':put'
          AND outbox.upload_id = upload.id
          AND outbox.account_id = upload.account_id
          AND outbox.operation = 'put'
          AND outbox.state = 'completed'
          AND outbox.attempt_count >= 1
          AND outbox.last_error IS NULL
          AND outbox.next_attempt_at_ms = outbox.completed_at_ms
          AND outbox.updated_at_ms = outbox.completed_at_ms
          AND outbox.created_at_ms <= outbox.completed_at_ms
          AND outbox.completed_at_ms IS NOT NULL
     )
) THEN 0 ELSE 1 END;

DROP TABLE temp.jobs_account_object_backfill_validation;
