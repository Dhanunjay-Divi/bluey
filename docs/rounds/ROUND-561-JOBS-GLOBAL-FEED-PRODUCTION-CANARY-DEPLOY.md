# Round 561 - Jobs Global Feed Production Canary Deploy

Date: 2026-07-21

## Scope

This round records the completed production rollout of the hardened global
candidate-feed pipeline from commit
`7891fd4f3e1101a362ca738c17e29dab460bb220`.

Only the Ashby and Lever source families were enabled for this canary. The
rollout does not enable model resume generation, local Browser distribution,
cloud Browser distribution, or unattended submission. Feed records remain
discovery leads until Bluey revalidates the original employer posting and
selects a certified execution path.

## Build And Deploy Identity

| Artifact | Identity |
| --- | --- |
| Reviewed source commit | `7891fd4f3e1101a362ca738c17e29dab460bb220` |
| Source archive | `/tmp/bluey-build-jobs-7891fd4f.tar.gz` |
| Source archive SHA-256 | `3016c73d02da8775383c8a19ca77aa249fb02846fa668e3aa45fc6d82691de65` |
| Installed source | `/opt/bluey-build-jobs-7891fd4f` |
| Jobs API binary SHA-256 | `26701f1650d6ac948751699813ec3840d29674ea6eb0d5293269571b0602cd26` |
| Global worker SHA-256 | `7a382e73782ccc0c37224f26181a94d20c3053c87a91e88895b012cf97eb134a` |
| Jobs portal index SHA-256 | `aee4388455b002a7cb08d56cb9055d35a922a924cccbb074dbc67dbd685c8d58` |

The portal was not rebuilt or replaced in this rollout.

## Production Canary Result

The worker completed both allowlisted source families with exact manifest
counts:

| Source family | Rows | Batches | Final health |
| --- | ---: | ---: | --- |
| Ashby | 46,180 | 93 | healthy |
| Lever | 68,029 | 137 | healthy |
| Total completed run rows | 114,209 | 230 | completed |

Canonical deduplication collapsed overlapping Ashby and Lever records to
101,709 unique publishable candidates. Only active memberships whose
`last_seen_run_id` belongs to a completed run for the same source are
publishable.

The fresh Lever retry completed under the deployed worker. The earlier
`artifact_invalid` status belonged to a failed historical run and was cleared
from source health only after the complete replacement snapshot committed.
The deployed worker remained active with `NRestarts=0`.

## Account Projection

The global index is shared and is not copied wholesale into every account.
When an account opens or refreshes its Jobs workspace, Bluey:

1. considers the newest 2,000 candidates inside the account's posting-age
   window;
2. applies active Career Track, role, location, employment, authorization,
   company, and other account filters;
3. ranks eligible candidates for that account; and
4. keeps a bounded queue of up to 500 matching jobs.

This supports hundreds of continuously refreshed matches without returning a
multi-million-row catalog to one browser. External leads stay Review first
until original-source verification succeeds.

## Runtime And Edge Verification

After both canaries completed:

```text
bluey-api.service
  active/running, NRestarts=0

bluey-jobs-api.service
  active/running, NRestarts=0
  /health commit = 7891fd4f3e1101a362ca738c17e29dab460bb220

bluey-jobs-global-discovery.service
  active/running, NRestarts=0

https://bluey.sh/jobs/                         200
https://bluey.sh/api/jobs/workspace            401 without authentication
https://bluey.sh/api/jobs/internal/discovery/lease
                                                 404 on the public hostname
```

Protected feature flags remained disabled:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## Rollback Evidence

The pre-rollout PostgreSQL backup is:

```text
/var/backups/bluey-api/round560-20260721T135921Z/
  bluey-postgres-20260721T135921Z.pgdump
```

It is 371,117,503 bytes, has 460 verified restore-list entries, and SHA-256
`cf76e46a3a8e34c21b83aeecfeae1af5da37ee3c295ed9868462269c1b64b0a6`.
The prior Jobs API binary in the same rollback set has SHA-256
`ed15586f1d2429c40f2604d2ff0de79016541c0de03c570563c105e991ccf66b`.

Rollback requires stopping the Jobs API and global worker, restoring the
paired database and prior binary/configuration set, restoring the previous
`/opt/bluey-jobs-global-discovery/current` target, and then re-running health,
edge, protected-route, and flag checks. Do not run an old binary against the
newer authority schema without the paired database rollback.

## Product Boundary

The pinned manifest reports 49 source families, 47 non-empty families,
5,096,589 raw rows, and 4,458,802 canonical candidate leads. This canary proves
the shared ingestion, completion, deduplication, account projection, and
recovery path for Ashby and Lever at production scale. It does not certify all
47 families or make every site safe for automatic submission.

Adding a source family requires a staged canary with manifest-count,
original-URL, freshness, deduplication, and error-rate evidence. Submission
requires an independently certified adapter, current-policy and candidate-fact
rechecks, an immutable application packet, and a side-effect-safe receipt.

Resume tailoring remains evidence locked. Bluey may rewrite, reorder, shorten,
and emphasize verified experience while preserving contact details, employers,
titles, locations, dates, education, certifications, projects, and template
identity. Production AI resume generation remains disabled until provider-cost
reservation, evidence validation, finalization fencing, and rendered-document
QA are proven together.
