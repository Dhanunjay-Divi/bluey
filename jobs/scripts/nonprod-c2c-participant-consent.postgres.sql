-- Synthetic-only PostgreSQL consent writer. Do not register as a migration.
-- Requires the exact PostgreSQL fixture consent tables and derived head view.
-- Synthetic role mapping is installed by the disposable controller, never by callers.
CREATE TABLE fixture_consent_principals (
  db_role name PRIMARY KEY,
  account_id text NOT NULL REFERENCES public.accounts(id) ON DELETE CASCADE
);

CREATE FUNCTION fixture_consent_immutable() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $body$
BEGIN
  IF TG_OP='DELETE' AND
    NOT EXISTS (SELECT 1 FROM public.accounts a WHERE a.id=OLD.account_id) AND EXISTS (
      SELECT 1 FROM public.jobs_workflow_cleanup_hard_delete_cascade_tokens t
       WHERE t.account_id=OLD.account_id
    ) THEN
    RETURN OLD;
  END IF;
  RAISE EXCEPTION 'consent_immutable' USING ERRCODE='23514';
END
$body$;
CREATE TRIGGER fixture_participant_immutable BEFORE UPDATE OR DELETE
  ON public.jobs_c2c_fixture_participants FOR EACH ROW EXECUTE FUNCTION fixture_consent_immutable();
CREATE TRIGGER fixture_revision_immutable BEFORE UPDATE OR DELETE
  ON public.jobs_c2c_fixture_participant_consent FOR EACH ROW EXECUTE FUNCTION fixture_consent_immutable();

-- A hash anchors this private receipt's exact semantic tuple. It does not
-- authenticate real consent; this function is deliberately unmounted.
CREATE FUNCTION fixture_advance_participant_consent(
  p_track text, p_participant text, p_source text, p_request text, p_expected text,
  p_evidence text, p_state text, p_source_revision text, p_source_canonical text,
  p_source_transition text, p_track_transition text, p_track_hash text, p_expires bigint
) RETURNS text
LANGUAGE plpgsql VOLATILE SECURITY DEFINER SET search_path=pg_catalog,public,pg_temp AS $body$
DECLARE
  owner_id text;
  previous public.jobs_c2c_fixture_participant_consent%ROWTYPE;
  receipt public.jobs_c2c_fixture_participant_consent%ROWTYPE;
  request_digest text;
  transition_digest text;
  recorded bigint;
  next_revision bigint;
BEGIN
  IF current_setting('transaction_isolation') <> 'read committed' OR
    p_track IS NULL OR p_track !~ '^[a-z0-9][a-z0-9_-]{0,99}$' OR
    p_participant IS NULL OR p_participant !~ '^[a-z0-9][a-z0-9_-]{0,99}$' OR
    p_source IS NULL OR p_source !~ '^[a-z0-9][a-z0-9_-]{0,99}$' OR
    p_request IS NULL OR p_request !~ '^[a-z0-9][a-z0-9_-]{0,99}$' OR
    (p_expected IS NOT NULL AND p_expected !~ '^[0-9a-f]{64}$') OR
    p_evidence IS NULL OR p_evidence !~ '^[0-9a-f]{64}$' OR
    p_state IS NULL OR p_state NOT IN ('active','revoked') THEN
    RETURN 'invalid_request';
  END IF;
  IF p_state='active' AND (
    p_source_revision IS NULL OR p_source_revision !~ '^[0-9a-f]{64}$' OR
    p_source_canonical IS NULL OR p_source_canonical !~ '^[0-9a-f]{64}$' OR
    p_source_transition IS NULL OR p_source_transition !~ '^[0-9a-f]{64}$' OR
    p_track_transition IS NULL OR p_track_transition !~ '^[a-z0-9][a-z0-9_-]{0,99}$' OR
    p_track_hash IS NULL OR p_track_hash !~ '^[0-9a-f]{64}$' OR p_expires IS NULL OR p_expires<=0
  ) OR p_state='revoked' AND (
    p_source_revision IS NOT NULL OR p_source_canonical IS NOT NULL OR
    p_source_transition IS NOT NULL OR p_track_transition IS NOT NULL OR
    p_track_hash IS NOT NULL OR p_expires IS NOT NULL
  ) THEN
    RETURN 'invalid_request';
  END IF;
  SELECT p.account_id INTO owner_id FROM public.fixture_consent_principals p WHERE p.db_role=session_user;
  IF owner_id IS NULL THEN RETURN 'account_unavailable'; END IF;
  -- Preserve the existing Track order: discovery-account advisory before
  -- the first row lock. A fresh statement checks deletion after any wait.
  PERFORM pg_advisory_xact_lock_shared(hashtextextended('jobs-discovery-account:' || owner_id, 0));
  PERFORM 1 FROM public.accounts a WHERE a.id=owner_id FOR UPDATE;
  IF NOT FOUND OR EXISTS (SELECT 1 FROM public.account_deletion_intents i WHERE i.account_id=owner_id) THEN
    RETURN 'account_unavailable';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM public.jobs_c2c_fixture_participants p
    WHERE p.account_id=owner_id AND p.participant_id=p_participant) THEN RETURN 'participant_unavailable'; END IF;

  request_digest=encode(sha256(convert_to(jsonb_build_array('fixture-c2c-pg-request-v1',owner_id,
    p_track,p_participant,p_source,p_request,p_expected,p_evidence,p_state,p_source_revision,
    p_source_canonical,p_source_transition,p_track_transition,p_track_hash,p_expires)::text,'UTF8')),'hex');
  SELECT * INTO receipt FROM public.jobs_c2c_fixture_participant_consent e
   WHERE e.account_id=owner_id AND e.request_id=p_request;
  IF FOUND THEN
    IF receipt.request_sha256<>request_digest THEN RETURN 'request_conflict'; END IF;
    RETURN 'replayed:' || receipt.revision || ':' || receipt.transition_sha256;
  END IF;
  SELECT * INTO previous FROM public.jobs_c2c_fixture_participant_consent_heads h
   WHERE h.account_id=owner_id AND h.track_id=p_track AND h.participant_id=p_participant
     AND h.source_id=p_source AND h.purpose='c2c_plan_only';
  IF previous.transition_sha256 IS DISTINCT FROM p_expected THEN RETURN 'stale_predecessor'; END IF;
  IF previous.state='revoked' OR (previous.revision IS NULL AND p_state='revoked') THEN
    RETURN 'invalid_transition';
  END IF;
  recorded=floor(extract(epoch FROM clock_timestamp())*1000)::bigint;
  IF recorded<coalesce(previous.recorded_ms,0) THEN RETURN 'clock_unavailable'; END IF;
  IF p_state='active' THEN
    IF recorded>=p_expires THEN RETURN 'requested_window_closed'; END IF;
    IF EXISTS (SELECT 1 FROM public.jobs_c2c_fixture_participant_consent e
          WHERE e.account_id=owner_id AND e.track_id=p_track AND e.participant_id=p_participant
            AND e.source_id=p_source AND e.purpose='c2c_plan_only'
            AND e.state='active' AND e.evidence_sha256=p_evidence) THEN
      RETURN 'new_evidence_required';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM public.fixture_consent_track_current(
      owner_id,p_track,p_track_transition,p_track_hash)) THEN RETURN 'track_unavailable'; END IF;
    PERFORM 1 FROM public.jobs_c2c_source_rights_heads h
      JOIN public.jobs_c2c_source_rights_revisions r ON
        (r.account_id,r.source_subject_id,r.revision_id,r.canonical_sha256,r.revision_no)=
        (h.account_id,h.source_subject_id,h.revision_id,h.canonical_sha256,h.generation)
     WHERE h.account_id=owner_id AND h.source_subject_id=p_source
       AND h.revision_id=p_source_revision AND h.canonical_sha256=p_source_canonical
       AND h.transition_sha256=p_source_transition AND r.purpose='c2c_plan_only'
       AND r.state='approved' AND r.plan_only AND r.not_before_ms<=recorded
       AND recorded<r.expires_at_ms AND p_expires<=r.expires_at_ms
       AND NOT EXISTS (SELECT 1 FROM public.jobs_c2c_source_rights_revocations x
         WHERE x.account_id=h.account_id AND x.source_subject_id=h.source_subject_id)
       AND NOT EXISTS (SELECT 1 FROM public.jobs_c2c_source_rights_revisions n
         WHERE n.account_id=r.account_id AND n.source_subject_id=r.source_subject_id
         AND n.revision_no>r.revision_no)
     FOR SHARE OF h;
    IF NOT FOUND THEN RETURN 'source_unavailable'; END IF;
  ELSE
    p_source_revision=previous.source_revision_id;
    p_source_canonical=previous.source_canonical_sha256;
    p_source_transition=previous.source_transition_sha256;
    p_track_transition=previous.track_transition_id;
    p_track_hash=previous.track_input_sha256;
    p_expires=recorded;
  END IF;

  next_revision=coalesce(previous.revision,0)+1;
  transition_digest=encode(sha256(convert_to(jsonb_build_array('fixture-c2c-pg-transition-v1',
    owner_id,p_track,p_participant,p_source,next_revision,p_expected,request_digest,recorded,
    p_expires,p_source_revision,p_source_canonical,p_source_transition,p_track_transition,
    p_track_hash)::text,'UTF8')),'hex');
  INSERT INTO public.jobs_c2c_fixture_participant_consent VALUES(
    owner_id,p_track,p_participant,p_source,'c2c_plan_only',next_revision,previous.revision,p_expected,
    transition_digest,p_source_revision,p_source_canonical,p_source_transition,p_track_transition,
    p_track_hash,p_evidence,p_request,request_digest,p_state,'synthetic-fixture',recorded,p_expires);
  RETURN 'recorded:' || next_revision || ':' || transition_digest;
END
$body$;
REVOKE ALL ON FUNCTION fixture_advance_participant_consent(
  text,text,text,text,text,text,text,text,text,text,text,text,bigint
) FROM PUBLIC;
REVOKE ALL ON FUNCTION fixture_consent_track_current(text,text,text,text) FROM PUBLIC;
