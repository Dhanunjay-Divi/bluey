-- Target: PostgreSQL
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
  jsonb_build_object(
    'artifact_class', 'jobs_resume_source',
    'jobs_resume_source_asset_id', id,
    'file_name', file_name,
    'file_type', file_type,
    'media_type', media_type,
    'page_count', page_count,
    'retention_policy', 'account_lifetime_until_deletion'
  )::text,
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
  jsonb_build_object(
    'artifact_class', 'jobs_browser_profile_snapshot',
    'jobs_browser_profile_id', browser_profile_id,
    'generation', generation,
    'envelope_version', envelope_version,
    'retention_policy', 'account_lifetime_until_deletion'
  )::text,
  updated_at_ms,
  updated_at_ms,
  updated_at_ms,
  NULL
FROM jobs_browser_profile_snapshots
WHERE TRUE
ON CONFLICT (account_id, object_kind, logical_id) DO NOTHING;

DO $bluey_jobs_account_object_backfill$
BEGIN
  IF EXISTS (
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
          AND (
            upload.metadata_json::jsonb = jsonb_build_object(
              'artifact_class', 'jobs_resume_source',
              'jobs_resume_source_asset_id', source.id,
              'file_name', source.file_name,
              'file_type', source.file_type,
              'media_type', source.media_type,
              'page_count', source.page_count,
              'retention_policy', 'account_lifetime_until_deletion'
            )
            OR (
              upload.metadata_json::jsonb = jsonb_build_object(
                'artifact_class', 'jobs_resume_source',
                'jobs_resume_source_asset_id', source.id,
                'request_id', source.id,
                'profile_mode', upload.metadata_json::jsonb -> 'profile_mode',
                'base_profile_sha256',
                  upload.metadata_json::jsonb -> 'base_profile_sha256',
                'requested_profile_sha256',
                  upload.metadata_json::jsonb -> 'requested_profile_sha256',
                'replaces_source_asset_id',
                  upload.metadata_json::jsonb -> 'replaces_source_asset_id',
                'file_name', source.file_name,
                'file_type', source.file_type,
                'media_type', source.media_type,
                'page_count', source.page_count,
                'retention_policy', 'account_lifetime_until_deletion'
              )
              AND (
                upload.metadata_json::jsonb ->> 'profile_mode' IN (
                  'replace', 'merge_source'
                )
              )
              AND (
                jsonb_typeof(upload.metadata_json::jsonb -> 'base_profile_sha256') = 'null'
                OR (
                  jsonb_typeof(
                    upload.metadata_json::jsonb -> 'base_profile_sha256'
                  ) = 'string'
                  AND upload.metadata_json::jsonb ->> 'base_profile_sha256'
                        ~ '^[0-9a-f]{64}$'
                )
              )
              AND NOT (
                upload.metadata_json::jsonb ->> 'profile_mode' = 'merge_source'
                AND jsonb_typeof(
                  upload.metadata_json::jsonb -> 'base_profile_sha256'
                ) <> 'null'
              )
              AND (
                jsonb_typeof(
                  upload.metadata_json::jsonb -> 'requested_profile_sha256'
                ) = 'null'
                OR (
                  jsonb_typeof(
                    upload.metadata_json::jsonb -> 'requested_profile_sha256'
                  ) = 'string'
                  AND upload.metadata_json::jsonb ->> 'requested_profile_sha256'
                        ~ '^[0-9a-f]{64}$'
                )
              )
              AND (
                (
                  upload.metadata_json::jsonb ->> 'profile_mode' = 'replace'
                  AND jsonb_typeof(
                    upload.metadata_json::jsonb -> 'requested_profile_sha256'
                  ) = 'string'
                )
                OR (
                  upload.metadata_json::jsonb ->> 'profile_mode' = 'merge_source'
                  AND jsonb_typeof(
                    upload.metadata_json::jsonb -> 'requested_profile_sha256'
                  ) = 'null'
                )
              )
              AND (
                jsonb_typeof(
                  upload.metadata_json::jsonb -> 'replaces_source_asset_id'
                ) = 'null'
                OR (
                  jsonb_typeof(
                    upload.metadata_json::jsonb -> 'replaces_source_asset_id'
                  ) = 'string'
                  AND length(
                    upload.metadata_json::jsonb ->> 'replaces_source_asset_id'
                  ) > 0
                )
              )
            )
          )
     )
  ) OR EXISTS (
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
          AND (
            upload.metadata_json::jsonb = jsonb_build_object(
              'artifact_class', 'jobs_browser_profile_snapshot',
              'jobs_browser_profile_id', snapshot.browser_profile_id,
              'generation', snapshot.generation,
              'envelope_version', snapshot.envelope_version,
              'retention_policy', 'account_lifetime_until_deletion'
            )
            OR (
              upload.metadata_json::jsonb = jsonb_build_object(
                'artifact_class', 'jobs_browser_profile_snapshot',
                'jobs_browser_profile_id', snapshot.browser_profile_id,
                'jobs_application_id',
                  upload.metadata_json::jsonb -> 'jobs_application_id',
                'jobs_run_id', snapshot.writer_run_id,
                'generation', snapshot.generation,
                'expected_generation', snapshot.generation - 1,
                'writer_fence', snapshot.writer_fence,
                'envelope_version', snapshot.envelope_version,
                'retention_policy', 'account_lifetime_until_deletion'
              )
              AND jsonb_typeof(
                    upload.metadata_json::jsonb -> 'jobs_application_id'
                  ) = 'string'
              AND length(
                    upload.metadata_json::jsonb ->> 'jobs_application_id'
                  ) > 0
            )
          )
     )
  ) THEN
    RAISE EXCEPTION 'Jobs account-object backfill validation failed';
  END IF;
END
$bluey_jobs_account_object_backfill$;

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

DO $bluey_jobs_account_object_outbox_backfill$
BEGIN
  IF EXISTS (
    SELECT 1
      FROM object_uploads upload
     WHERE upload.state = 'ready'
       AND upload.session_id IS NULL
       AND upload.metadata_json::jsonb ->> 'artifact_class' IN (
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
  ) THEN
    RAISE EXCEPTION 'Jobs account-object PUT outbox backfill validation failed';
  END IF;
END
$bluey_jobs_account_object_outbox_backfill$;
