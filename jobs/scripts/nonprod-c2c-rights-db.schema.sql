-- Nonproduction fixture only. Never run from either server migration registry.
-- Use the existing account and Career Track tables in an owned synthetic database.
CREATE UNIQUE INDEX jobs_c2c_fixture_track_owner
    ON jobs_tracks(account_id, id);

CREATE TABLE jobs_c2c_fixture_source_rights (
    source_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    vendor_id TEXT NOT NULL,
    terms_sha256 TEXT NOT NULL CHECK (length(terms_sha256) = 64 AND trim(terms_sha256, '0123456789abcdef') = ''),
    evidence_sha256 TEXT NOT NULL CHECK (
        length(evidence_sha256) = 64 AND trim(evidence_sha256, '0123456789abcdef') = ''
    ),
    state TEXT NOT NULL CHECK (state IN ('approved', 'denied', 'expired', 'terms_changed', 'review_required')),
    permits_matching INTEGER NOT NULL CHECK (permits_matching IN (0, 1)),
    effective_at_ms BIGINT NOT NULL CHECK (effective_at_ms >= 0),
    expires_at_ms BIGINT NOT NULL CHECK (expires_at_ms > effective_at_ms),
    PRIMARY KEY (source_id, revision),
    UNIQUE (source_id, revision, vendor_id, terms_sha256)
);

CREATE TABLE jobs_c2c_fixture_requirements (
    requirement_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    source_id TEXT NOT NULL,
    source_revision BIGINT NOT NULL,
    vendor_id TEXT NOT NULL,
    terms_sha256 TEXT NOT NULL,
    engagement TEXT NOT NULL CHECK (engagement IN ('c2c', 'w2', '1099', 'unknown')),
    observed_at_ms BIGINT NOT NULL CHECK (observed_at_ms >= 0),
    verified_at_ms BIGINT NOT NULL CHECK (verified_at_ms >= observed_at_ms),
    expires_at_ms BIGINT NOT NULL CHECK (expires_at_ms > verified_at_ms),
    PRIMARY KEY (requirement_id, revision),
    UNIQUE (requirement_id, revision, vendor_id),
    FOREIGN KEY (source_id, source_revision, vendor_id, terms_sha256)
        REFERENCES jobs_c2c_fixture_source_rights(source_id, revision, vendor_id, terms_sha256)
);

CREATE TABLE jobs_c2c_fixture_grants (
    account_id TEXT NOT NULL,
    track_id TEXT NOT NULL,
    requirement_id TEXT NOT NULL,
    requirement_revision BIGINT NOT NULL,
    vendor_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    state TEXT NOT NULL CHECK (state IN ('active', 'revoked')),
    evidence_sha256 TEXT NOT NULL CHECK (
        length(evidence_sha256) = 64 AND trim(evidence_sha256, '0123456789abcdef') = ''
    ),
    effective_at_ms BIGINT NOT NULL CHECK (effective_at_ms >= 0),
    expires_at_ms BIGINT NOT NULL CHECK (expires_at_ms > effective_at_ms),
    PRIMARY KEY (account_id, track_id, requirement_id, revision),
    FOREIGN KEY (account_id, track_id) REFERENCES jobs_tracks(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (requirement_id, requirement_revision, vendor_id)
        REFERENCES jobs_c2c_fixture_requirements(requirement_id, revision, vendor_id)
);
