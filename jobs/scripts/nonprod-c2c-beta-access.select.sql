-- Advisory fixture only. $1 is the trusted account from the caller's principal
-- mapping, never a body/query parameter. The caller must separately enforce
-- the process master switch. A match is not participant or effect authority.
SELECT 1 AS existing_beta_access
  FROM accounts AS account
  JOIN jobs_public_beta_enrollments AS enrollment ON enrollment.account_id = account.id
  JOIN jobs_public_beta_cohorts AS cohort ON cohort.id = enrollment.cohort_id
 WHERE account.id = $1
   AND cohort.id = 'public-v1'
   AND cohort.state IN ('open', 'closed_to_new')
   AND account.email_verified_at IS NOT NULL
   AND account.is_temporary = 0
   AND NOT EXISTS (
     SELECT 1 FROM jobs_public_beta_overrides AS gate_override
      WHERE gate_override.cohort_id = cohort.id AND gate_override.account_id = account.id
        AND gate_override.denied <> 0
   )
   AND NOT EXISTS (
     SELECT 1 FROM account_deletion_intents AS deletion WHERE deletion.account_id = account.id
   );
