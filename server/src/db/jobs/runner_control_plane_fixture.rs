// Debug/integration-feature-only fixtures. The measured issuer requires external
// qualified artifacts and signed authority; it never executes or preclaims a runner.
// Historical synthetic bootstrap evidence below is not startup or deployment proof.
use super::*;

/// A separate, externally authorized startup issuer. No account, job, volume key,
/// claimed instance or successful runtime observation is accepted as input.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredRunnerStartupRequest {
    pub version: u8,
    pub operation: String,
    pub worker_id: String,
    pub issuance_ref: String,
    pub provider_resource_id: String,
    pub resource_fingerprint: String,
    pub authorization_ref: String,
    pub ttl_ms: i64,
    pub process_runtime: RunnerProcessRuntimeAttestation,
    pub startup_evidence_base64url: String,
    pub release_test_evidence_base64url: String,
    pub trust_policy: ManagedCloudAuthorityEnvelope,
    pub release: ManagedCloudReleaseImportRequest,
    pub cohort: ManagedCloudCohortImportRequest,
    pub activation: ManagedCloudActivationImportRequest,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredRunnerStartupResult {
    pub version: u8,
    pub operation: &'static str,
    pub worker_id: String,
    pub scope: ManagedCloudScope,
    pub managed_runtime_grant: ManagedCloudRuntimeGrant,
    pub volume_admission_grant: RunnerVolumeAdmissionGrant,
    pub volume_admission_token: String,
    pub process_runtime_grant: RunnerProcessRuntimeGrant,
    pub process_runtime_token: String,
    pub d1_export_manifest_sha256: String,
    pub d1_qualification_sha256: String,
    pub d1_controller_journal_sha256: String,
    pub grants_issued: u8,
    pub grants_consumed: u8,
    pub runtime_observed: bool,
    pub customer_admission_proven: bool,
    pub release_authority: bool,
}

struct MeasuredRunnerAuthority {
    manifest_sha256: String,
    activation_sha256: String,
    runtime_identity_sha256: String,
    expires_at_ms: i64,
}

/// Bytes read from the exact retained D1 handoff, not a caller success flag.
/// The CLI streams the archive separately; its bounded digest/size are supplied here.
pub struct MeasuredRunnerD1Readback {
    pub export_started: Vec<u8>,
    pub export_manifest: Vec<u8>,
    pub qualification: Vec<u8>,
    pub inventory: Vec<u8>,
    pub measurement: Vec<u8>,
    pub inspection: Vec<u8>,
    pub controller_journal: Vec<u8>,
    pub controller_final_stdout: Vec<u8>,
    pub external_terminal: Vec<u8>,
    pub archive_sha256: String,
    pub archive_size: u64,
}

/// Independently reviewed parent admission, never constructed from D1 report fields.
/// These expected bytes select one source/tool closure; they do not certify execution.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredRunnerD1Expected {
    pub version: u8,
    pub source_commit: String,
    pub source_branch: String,
    pub source_file_count: u64,
    pub source_manifest_sha256: String,
    pub source_archive_sha256: String,
    pub controller_source_sha256: String,
    pub harness_sha256: String,
    pub vm_test_sha256: String,
    pub artifact_helper_sha256: String,
    pub inspector_sha256: String,
    pub image_tool_sources_sha256: String,
    pub inspection_tool_sources_sha256: String,
}

impl MeasuredRunnerD1Expected {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.version == 1
                && self.source_commit.len() == 40
                && self.source_commit != "0".repeat(40)
                && self
                    .source_commit
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
                && (1..=100_000).contains(&self.source_file_count)
                && !self.source_branch.is_empty()
                && self.source_branch.len() <= 255
                && self.source_branch.split('/').all(|part| part
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                    && !part.contains("..")
                    && !part.ends_with('.')
                    && !part.ends_with(".lock")
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))),
            "measured_d1_expected_source"
        );
        for digest in [
            &self.source_manifest_sha256,
            &self.source_archive_sha256,
            &self.controller_source_sha256,
            &self.harness_sha256,
            &self.vm_test_sha256,
            &self.artifact_helper_sha256,
            &self.inspector_sha256,
            &self.image_tool_sources_sha256,
            &self.inspection_tool_sources_sha256,
        ] {
            anyhow::ensure!(
                managed_cloud_hex64(digest) && digest != &"0".repeat(64),
                "measured_d1_expected_digest"
            );
        }
        Ok(())
    }
}

pub struct MeasuredRunnerD1Binding {
    source_commit: String,
    inventory_sha256: String,
    measurement_sha256: String,
    measurement: ManagedCloudRuntimeMeasurementAuthority,
    process_runtime: RunnerProcessRuntimeAttestation,
    export_manifest_sha256: String,
    export_started_sha256: String,
    qualification_sha256: String,
    controller_journal_sha256: String,
    controller_final_stdout_sha256: String,
    external_terminal_sha256: String,
}

fn d1_keys(value: &Value, keys: &[&str]) -> Result<()> {
    let object = value.as_object().context("measured D1 object missing")?;
    anyhow::ensure!(
        object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key)),
        "measured_d1_closed_shape"
    );
    Ok(())
}

fn d1_json(bytes: &[u8], maximum: usize, newline: bool) -> Result<Value> {
    anyhow::ensure!(
        !bytes.is_empty() && bytes.len() <= maximum && bytes.is_ascii(),
        "measured_d1_bytes"
    );
    let value: Value = serde_json::from_slice(bytes)?;
    let mut canonical = managed_cloud_canonical_json(&value)?;
    if !newline {
        let _ = canonical.pop();
    }
    anyhow::ensure!(canonical == bytes, "measured_d1_canonical");
    Ok(value)
}

fn d1_text(value: &Value) -> Result<&str> {
    value.as_str().context("measured D1 string missing")
}

fn d1_digest(value: &Value) -> Result<&str> {
    let digest = d1_text(value)?;
    anyhow::ensure!(
        managed_cloud_hex64(digest) && digest != "0".repeat(64),
        "measured_d1_digest"
    );
    Ok(digest)
}

fn validate_startup_release_test_evidence(value: &Value, source_commit: &str) -> Result<()> {
    // Preserve the normal release-v3 test-evidence-v2 meaning. These are imported
    // signed metadata bindings, not an assertion that this issuer ran remote CI.
    d1_keys(
        value,
        &[
            "version",
            "audience",
            "sourceCommit",
            "repository",
            "repositoryId",
            "defaultBranch",
            "releaseContractSelfTestSha256",
            "ciRuns",
        ],
    )?;
    let repository = d1_text(&value["repository"])?;
    let repository_parts: Vec<_> = repository.split('/').collect();
    let branch = d1_text(&value["defaultBranch"])?;
    anyhow::ensure!(
        value["version"] == 2
            && value["audience"] == "bluey-jobs-managed-cloud-test-evidence-v2"
            && value["sourceCommit"] == source_commit
            && repository_parts.len() == 2
            && repository_parts.iter().all(|part| !part.is_empty()
                && part.len() <= 100
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte)))
            && value["repositoryId"]
                .as_i64()
                .is_some_and(|number| managed_cloud_safe_integer(number, true))
            && !branch.is_empty()
            && branch.len() <= 255
            && branch.trim() == branch
            && branch.is_ascii()
            && !branch
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte == b' '),
        "measured_startup_release_test_evidence"
    );
    d1_digest(&value["releaseContractSelfTestSha256"])?;
    let runs = value["ciRuns"]
        .as_array()
        .context("measured startup CI metadata missing")?;
    anyhow::ensure!(runs.len() == 2, "measured_startup_release_test_evidence");
    for (run, (check_id, workflow)) in runs.iter().zip([
        ("jobs-ci", ".github/workflows/jobs-ci.yml"),
        (
            "observability-policy",
            ".github/workflows/observability-policy.yml",
        ),
    ]) {
        d1_keys(
            run,
            &[
                "checkId",
                "conclusion",
                "event",
                "headBranch",
                "headSha",
                "repositoryId",
                "runAttempt",
                "runId",
                "workflowPath",
            ],
        )?;
        anyhow::ensure!(
            run["checkId"] == check_id
                && run["workflowPath"] == workflow
                && run["conclusion"] == "success"
                && run["event"] == "push"
                && run["headBranch"] == branch
                && run["headSha"] == source_commit
                && run["repositoryId"] == value["repositoryId"]
                && ["runId", "runAttempt"].iter().all(|key| run[key]
                    .as_i64()
                    .is_some_and(|number| managed_cloud_safe_integer(number, true))),
            "measured_startup_release_test_evidence"
        );
    }
    anyhow::ensure!(
        runs[0]["runId"] != runs[1]["runId"],
        "measured_startup_release_test_evidence"
    );
    Ok(())
}

fn validate_measured_d1_controller(
    input: &MeasuredRunnerD1Readback,
    expected: &MeasuredRunnerD1Expected,
    manifest: &Value,
    qualification: &Value,
) -> Result<()> {
    anyhow::ensure!(
        !input.controller_journal.is_empty()
            && input.controller_journal.len() <= 4 * 1024 * 1024
            && input.controller_journal.ends_with(b"\n")
            && input.controller_final_stdout.len() <= 65_536
            && !input.controller_final_stdout.is_empty()
            && input.external_terminal.len() <= 4096,
        "measured_d1_controller_bounds"
    );
    let terminal: Value = serde_json::from_slice(&input.external_terminal)?;
    d1_keys(
        &terminal,
        &[
            "version",
            "controllerSourceSha256",
            "controllerFinalStdoutSha256",
            "controllerJournalSha256",
            "code",
            "signal",
        ],
    )?;
    anyhow::ensure!(
        input.external_terminal.len() <= 4096
            && terminal["version"] == 1
            && terminal["controllerSourceSha256"] == expected.controller_source_sha256
            && terminal["controllerFinalStdoutSha256"]
                == managed_cloud_sha256(&input.controller_final_stdout)
            && terminal["controllerJournalSha256"]
                == managed_cloud_sha256(&input.controller_journal)
            && terminal["code"] == 0
            && terminal["signal"].is_null(),
        "measured_d1_external_terminal"
    );
    anyhow::ensure!(
        input.controller_final_stdout.ends_with(b"\n")
            && input
                .controller_final_stdout
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
                == 1,
        "measured_d1_final_stdout_line"
    );
    let stdout: Value = serde_json::from_slice(&input.controller_final_stdout)?;
    d1_keys(
        &stdout,
        &[
            "event",
            "success",
            "journal",
            "journalSha256",
            "terminal",
            "states",
            "parentAbsent",
        ],
    )?;
    anyhow::ensure!(
        stdout["event"] == "phase639_image_controller_finished"
            && stdout["success"] == true
            && stdout["journalSha256"] == terminal["controllerJournalSha256"]
            && stdout["terminal"] == json!({"code":0,"signal":null})
            && stdout["parentAbsent"] == true,
        "measured_d1_controller_terminal"
    );
    let journal = std::str::from_utf8(&input.controller_journal)?;
    let lines: Vec<_> = journal.lines().collect();
    anyhow::ensure!(
        lines.len() <= 512
            && lines
                .iter()
                .all(|line| !line.is_empty() && line.len() <= 128 * 1024),
        "measured_d1_journal_bounds"
    );
    let records: Vec<Value> = lines
        .iter()
        .map(|line| serde_json::from_str(line))
        .collect::<std::result::Result<_, _>>()?;
    let one = |event: &str| -> Result<&Value> {
        let mut matches = records.iter().filter(|record| record["event"] == event);
        let value = matches
            .next()
            .context("measured D1 journal stage missing")?;
        anyhow::ensure!(matches.next().is_none(), "measured_d1_duplicate_stage");
        Ok(value)
    };
    let allowed = [
        "source_before",
        "dispatch",
        "owned_vm_state_observed",
        "image_progress",
        "command_terminal",
        "managed_runner_vm_image_smoke_finished",
        "residue",
        "source_after",
        "retained_artifact_proof",
        "source_after_artifacts",
        "controller_finished",
    ];
    anyhow::ensure!(
        records
            .iter()
            .all(|record| allowed.iter().any(|name| record["event"] == *name))
            && records
                .first()
                .is_some_and(|record| record["event"] == "source_before")
            && records
                .last()
                .is_some_and(|record| record["event"] == "controller_finished")
            && one("controller_finished")?["success"] == true,
        "measured_d1_journal_sequence"
    );
    // Exact success record emitted by the independently admitted controller.
    // A successful outer wrapper cannot override a failed or authority-widened
    // controller record inside its hash-bound journal.
    let finished = one("controller_finished")?;
    d1_keys(
        finished,
        &[
            "event",
            "time",
            "success",
            "d1Qualified",
            "qualified",
            "d2Authority",
            "releaseAuthority",
            "actualTerminal",
            "failure",
            "originalFailure",
            "failedArchiveRetained",
            "elapsedMs",
        ],
    )?;
    anyhow::ensure!(
        finished["success"] == true
            && finished["d1Qualified"] == true
            && finished["qualified"] == false
            && finished["d2Authority"] == false
            && finished["releaseAuthority"] == false
            && finished["actualTerminal"] == json!({"code":0,"signal":null})
            && finished["failure"].is_null()
            && finished["originalFailure"].is_null()
            && finished["failedArchiveRetained"] == false
            && finished["elapsedMs"]
                .as_u64()
                .is_some_and(|elapsed| elapsed <= 9_007_199_254_740_991)
            && finished["time"]
                .as_str()
                .is_some_and(|time| time.len() == 24
                    && chrono::DateTime::parse_from_rfc3339(time)
                        .is_ok_and(
                            |parsed| parsed.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string() == time
                        )),
        "measured_d1_controller_finished"
    );
    let before = one("source_before")?;
    for event in ["source_after", "source_after_artifacts"] {
        let after = one(event)?;
        for key in ["head", "branch", "files", "manifest", "rawStatus"] {
            anyhow::ensure!(after[key] == before[key], "measured_d1_source_changed");
        }
    }
    anyhow::ensure!(
        before["head"] == manifest["source"]["sourceCommit"]
            && before["head"] == expected.source_commit
            && before["branch"] == expected.source_branch
            && before["files"] == expected.source_file_count
            && before["manifest"] == expected.source_manifest_sha256
            && before["selfSha256"] == expected.controller_source_sha256,
        "measured_d1_controller_source"
    );
    let progress: Vec<_> = records
        .iter()
        .filter(|record| record["event"] == "image_progress")
        .map(|record| record["phase"].as_str())
        .collect();
    anyhow::ensure!(
        progress
            == [
                Some("vm_starting"),
                Some("vm_isolation_checked"),
                Some("inputs_prepared"),
                Some("builder_started"),
                Some("image_build_started"),
                Some("image_build_finished"),
                Some("image_inspection_started"),
                Some("image_inspection_finished")
            ],
        "measured_d1_progress"
    );
    let command = one("command_terminal")?;
    anyhow::ensure!(
        command["code"] == 0
            && command["signal"].is_null()
            && command["overflow"] == false
            && command["observerFailure"].is_null(),
        "measured_d1_launcher_terminal"
    );
    let residue = one("residue")?;
    let states = residue["states"]
        .as_array()
        .context("measured D1 state proof missing")?;
    anyhow::ensure!(
        states.len() == 1
            && states[0]["absent"] == true
            && residue["parentEmpty"] == true
            && residue["parentAbsent"] == true
            && residue["preexistingStateCount"] == 0
            && stdout["states"] == residue["states"],
        "measured_d1_cleanup"
    );
    let retained = one("retained_artifact_proof")?;
    anyhow::ensure!(
        retained["manifestSha256"] == managed_cloud_sha256(&input.export_manifest)
            && retained["qualificationSha256"] == managed_cloud_sha256(&input.qualification)
            && retained["sourceCommit"] == before["head"]
            && retained["d2Authority"] == false
            && retained["releaseAuthority"] == false,
        "measured_d1_retained_proof"
    );
    let retained_files = retained["files"]
        .as_array()
        .context("measured D1 retained files missing")?;
    let manifest_files = manifest["files"]
        .as_array()
        .context("measured D1 manifest files missing")?;
    anyhow::ensure!(
        retained_files.len() == 4 && manifest_files.len() == 4,
        "measured_d1_retained_proof"
    );
    for (retained_file, saved) in retained_files.iter().zip(manifest_files) {
        d1_keys(retained_file, &["name", "size", "sha256", "identity"])?;
        d1_keys(&retained_file["identity"], &["dev", "ino"])?;
        anyhow::ensure!(
            ["name", "size", "sha256"]
                .iter()
                .all(|key| retained_file[key] == saved[key])
                && ["dev", "ino"]
                    .iter()
                    .all(|key| retained_file["identity"][key] == saved["identity"][key]),
            "measured_d1_retained_file_changed"
        );
    }
    let report = one("managed_runner_vm_image_smoke_finished")?;
    anyhow::ensure!(
        report["version"] == 1
            && report["qualification"] == "isolated_measured_runner_image_only"
            && report["claims"]
                == json!({"normalRunnerStartup":false,"managedAuthority":false,"submission":false,"production":false})
            && report["compatibility"]["baseCommit"] == before["head"]
            && report["compatibility"]["harnessSha256"] == expected.harness_sha256
            && report["compatibility"]["testSha256"] == expected.vm_test_sha256
            && report["compatibility"]["cleanup"] == qualification["proof"]["cleanup"]
            && report["imageToolSourcesSha256"] == expected.image_tool_sources_sha256
            && report["image"]["sourceCommit"] == before["head"]
            && report["image"]["sourceArchiveSha256"] == manifest["source"]["sourceArchiveSha256"]
            && report["image"]["toolSourcesSha256"] == manifest["source"]["toolSourcesSha256"]
            && report["image"]["inspection"] == manifest["inspection"]
            && report["image"]["cleanup"] == json!({"builderRemoved":true,"containerRemoved":true}),
        "measured_d1_final_report"
    );
    // The pinned controller adds only its second `time` property when journaling
    // this report. Remove that exact prefix field without sorting/reserializing
    // the report: qualification hashes the original emitted bytes plus newline.
    let ordinal = records
        .iter()
        .position(|record| record["event"] == "managed_runner_vm_image_smoke_finished")
        .context("measured D1 report missing")?;
    let event = "{\"event\":\"managed_runner_vm_image_smoke_finished\"";
    let prefix = format!(
        "{event},\"time\":{}",
        serde_json::to_string(&report["time"])?
    );
    let suffix = lines[ordinal]
        .strip_prefix(&prefix)
        .context("measured D1 report encoding changed")?;
    anyhow::ensure!(suffix.starts_with(','), "measured_d1_report_encoding");
    let original = format!("{event}{suffix}\n");
    anyhow::ensure!(
        qualification["proof"]["imageReportSha256"] == managed_cloud_sha256(original.as_bytes()),
        "measured_d1_emitted_report_changed"
    );
    Ok(())
}

/// Revalidates retained metadata/content. This never grants D2, fleet or release authority.
pub fn validate_measured_runner_d1(
    input: &MeasuredRunnerD1Readback,
    admission: &MeasuredRunnerD1Expected,
    declared_runner_build_id: &str,
) -> Result<MeasuredRunnerD1Binding> {
    admission.validate()?;
    anyhow::ensure!(
        runner_build_id_is_canonical(declared_runner_build_id),
        "measured_d1_deployment_build_token"
    );
    let manifest = d1_json(&input.export_manifest, 65_536, false)?;
    let qualification = d1_json(&input.qualification, 65_536, false)?;
    let started = d1_json(&input.export_started, 65_536, false)?;
    let inspection = d1_json(&input.inspection, 16_384, true)?;
    let measurement: ManagedCloudRuntimeMeasurementAuthority =
        managed_cloud_parse_canonical(&input.measurement)?;
    d1_keys(
        &manifest,
        &[
            "version",
            "state",
            "qualified",
            "d2Authority",
            "releaseAuthority",
            "binding",
            "source",
            "reader",
            "files",
            "inspection",
        ],
    )?;
    d1_keys(
        &started,
        &[
            "version",
            "state",
            "qualified",
            "d2Authority",
            "releaseAuthority",
            "binding",
            "source",
            "artifacts",
        ],
    )?;
    d1_keys(
        &qualification,
        &[
            "version",
            "state",
            "qualification",
            "d2Authority",
            "releaseAuthority",
            "externalTerminalZeroRequired",
            "exportManifestSha256",
            "proof",
        ],
    )?;
    let manifest_sha256 = managed_cloud_sha256(&input.export_manifest);
    let qualification_sha256 = managed_cloud_sha256(&input.qualification);
    anyhow::ensure!(
        manifest["version"] == 1
            && manifest["state"] == "attachments_complete"
            && manifest["qualified"] == false
            && started["version"] == 1
            && started["state"] == "incomplete"
            && started["qualified"] == false
            && qualification["version"] == 1
            && qualification["state"] == "d1_qualified"
            && qualification["qualification"] == "isolated_measured_runner_image_only"
            && qualification["externalTerminalZeroRequired"] == true
            && qualification["exportManifestSha256"] == manifest_sha256
            && [&manifest, &started, &qualification]
                .iter()
                .all(|item| item["d2Authority"] == false && item["releaseAuthority"] == false)
            && started["binding"] == manifest["binding"]
            && started["source"] == manifest["source"]
            && manifest["inspection"] == inspection,
        "measured_d1_receipt_binding"
    );
    let source = &manifest["source"];
    d1_keys(
        &manifest["binding"],
        &[
            "parentIdentity",
            "childIdentity",
            "childName",
            "hostRuntime",
        ],
    )?;
    d1_keys(
        &manifest["reader"],
        &["runtime", "readerSourceSha256", "continuity"],
    )?;
    d1_keys(
        source,
        &[
            "sourceCommit",
            "sourceArchiveSha256",
            "toolSourcesSha256",
            "harnessSha256",
            "imageToolSourcesSha256",
        ],
    )?;
    let commit = d1_text(&source["sourceCommit"])?;
    anyhow::ensure!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')),
        "measured_d1_source"
    );
    for key in [
        "sourceArchiveSha256",
        "toolSourcesSha256",
        "harnessSha256",
        "imageToolSourcesSha256",
    ] {
        d1_digest(&source[key])?;
    }
    anyhow::ensure!(
        commit == admission.source_commit
            && source["sourceArchiveSha256"] == admission.source_archive_sha256
            && source["harnessSha256"] == admission.harness_sha256
            && source["imageToolSourcesSha256"] == admission.image_tool_sources_sha256
            && source["toolSourcesSha256"] == admission.inspection_tool_sources_sha256
            && manifest["reader"]["readerSourceSha256"] == admission.artifact_helper_sha256,
        "measured_d1_expected_closure"
    );
    let proof = &qualification["proof"];
    d1_keys(
        proof,
        &[
            "sourceCommit",
            "imageReportSha256",
            "harnessSha256",
            "toolSourcesSha256",
            "postListenerChecked",
            "sourceUnchanged",
            "cleanup",
        ],
    )?;
    let cleanup_keys = [
        "privateInventoriesEmpty",
        "ownedVmPidsAbsent",
        "socketAbsent",
        "stateAbsent",
        "usernetSocketsAbsent",
    ];
    d1_keys(&proof["cleanup"], &cleanup_keys)?;
    anyhow::ensure!(
        proof["sourceCommit"] == source["sourceCommit"]
            && proof["harnessSha256"] == source["harnessSha256"]
            && proof["toolSourcesSha256"] == source["imageToolSourcesSha256"]
            && proof["postListenerChecked"] == true
            && proof["sourceUnchanged"] == true
            && cleanup_keys.iter().all(|key| proof["cleanup"][key] == true),
        "measured_d1_qualification"
    );
    let names = [
        "runner.oci.tar",
        "inventory.json",
        "measurement.json",
        "report.json",
    ];
    let sizes = [
        input.archive_size,
        input.inventory.len() as u64,
        input.measurement.len() as u64,
        input.inspection.len() as u64,
    ];
    let hashes = [
        input.archive_sha256.clone(),
        managed_cloud_sha256(&input.inventory),
        managed_cloud_sha256(&input.measurement),
        managed_cloud_sha256(&input.inspection),
    ];
    let caps = [
        4 * 1024_u64.pow(3),
        32 * 1024_u64.pow(2),
        128 * 1024,
        16 * 1024,
    ];
    let files = manifest["files"]
        .as_array()
        .context("measured D1 files missing")?;
    anyhow::ensure!(
        files.len() == 4 && started["artifacts"] == json!(names),
        "measured_d1_file_set"
    );
    for (index, file) in files.iter().enumerate() {
        d1_keys(file, &["name", "size", "sha256", "identity"])?;
        anyhow::ensure!(
            file["name"] == names[index]
                && file["size"].as_u64() == Some(sizes[index])
                && sizes[index] > 0
                && sizes[index] <= caps[index]
                && d1_digest(&file["sha256"])? == hashes[index],
            "measured_d1_attachment_changed"
        );
        // Original identities remain historical receipt bindings. A guest copy is
        // content-equivalent, not inode-equivalent; the CLI binds its current FDs.
        d1_keys(
            &file["identity"],
            &[
                "dev", "ino", "uid", "mode", "nlink", "size", "mtimeNs", "ctimeNs",
            ],
        )?;
        anyhow::ensure!(
            file["identity"]["size"] == file["size"] && file["identity"]["nlink"] == 1,
            "measured_d1_original_identity"
        );
    }
    d1_keys(
        &inspection,
        &[
            "version",
            "expected",
            "oci",
            "runtime",
            "native",
            "browser",
            "claims",
            "inspectorSha256",
        ],
    )?;
    let expected = &inspection["expected"];
    d1_keys(
        expected,
        &[
            "version",
            "sourceCommit",
            "buildId",
            "configSchemaSha256",
            "migrationSetSha256",
            "protocolSetSha256",
        ],
    )?;
    d1_keys(
        &inspection["oci"],
        &["manifestSha256", "imageConfigSha256", "inventorySha256"],
    )?;
    d1_keys(
        &inspection["runtime"],
        &[
            "runtimeMeasurementSha256",
            "runtimeIdentitySha256",
            "automationBundleSha256",
        ],
    )?;
    d1_digest(&inspection["inspectorSha256"])?;
    anyhow::ensure!(
        inspection["inspectorSha256"] == admission.inspector_sha256,
        "measured_d1_expected_inspector"
    );
    for (object, keys) in [
        (
            &inspection["oci"],
            &["manifestSha256", "imageConfigSha256", "inventorySha256"][..],
        ),
        (
            &inspection["runtime"],
            &[
                "runtimeMeasurementSha256",
                "runtimeIdentitySha256",
                "automationBundleSha256",
            ][..],
        ),
        (
            expected,
            &[
                "configSchemaSha256",
                "migrationSetSha256",
                "protocolSetSha256",
            ][..],
        ),
    ] {
        for key in keys {
            d1_digest(&object[key])?;
        }
    }
    let runtime_identity =
        managed_cloud_runtime_identity_sha256(&hashes[2], "jobs-runner", "managed_runner")?;
    anyhow::ensure!(
        inspection["version"] == 1
            && expected["version"] == 1
            && expected["sourceCommit"] == commit
            && expected["buildId"] == format!("nonprod-p639-{}", &commit[..12])
            && measurement.source_commit == commit
            && measurement.component_id == "jobs-runner"
            && measurement.roles == ["managed_runner"]
            && expected["buildId"] == measurement.build_id
            && expected["configSchemaSha256"] == measurement.config_schema_sha256
            && expected["migrationSetSha256"] == measurement.migration_set_sha256
            && expected["protocolSetSha256"] == measurement.protocol_set_sha256
            && inspection["oci"]["inventorySha256"] == hashes[1]
            && inspection["runtime"]["runtimeMeasurementSha256"] == hashes[2]
            && inspection["runtime"]["runtimeIdentitySha256"] == runtime_identity
            && inspection["oci"]["manifestSha256"] != inspection["oci"]["imageConfigSha256"]
            && inspection["native"]
                == json!({"writeReadback":true,"inventory":true,"removed":true})
            && inspection["claims"]
                == json!({"actualImageInspection":true,"nativeAddon":true,"browserOnly":true,
            "normalRunnerStartup":false,"managedAuthority":false,"submission":false,"production":false}),
        "measured_d1_inspection_binding"
    );
    let automation_entries: Vec<_> = measurement
        .measured_files
        .iter()
        .filter(|item| item.path.starts_with("app/automation/"))
        .map(|item| json!({"path":item.path,"sha256":item.sha256}))
        .collect();
    anyhow::ensure!(
        [
            "app/automation/package.json",
            "app/automation/dist/index.js"
        ]
        .iter()
        .all(|path| automation_entries.iter().any(|item| item["path"] == *path)),
        "measured_d1_automation_missing"
    );
    let bundle_sha256 = managed_cloud_digest(
        &json!({"audience":"bluey-jobs-managed-runner-automation-bundle-v1",
        "entries":automation_entries,"version":1}),
    )?;
    anyhow::ensure!(
        inspection["runtime"]["automationBundleSha256"] == bundle_sha256,
        "measured_d1_automation_changed"
    );
    let browser = &inspection["browser"];
    d1_keys(
        browser,
        &[
            "playwrightVersion",
            "chromiumVersion",
            "chromiumRevision",
            "executablePath",
            "executableSha256",
            "titleReadback",
            "requestsAborted",
            "launchProfile",
        ],
    )?;
    let executable_path = d1_text(&browser["executablePath"])?;
    let relative_executable = executable_path
        .strip_prefix('/')
        .context("measured D1 executable path")?;
    for (key, minimum, maximum) in [("playwrightVersion", 3, 3), ("chromiumVersion", 2, 4)] {
        let parts: Vec<_> = d1_text(&browser[key])?.split('.').collect();
        anyhow::ensure!(
            (minimum..=maximum).contains(&parts.len())
                && parts
                    .iter()
                    .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
            "measured_d1_browser_version"
        );
    }
    let revision = d1_text(&browser["chromiumRevision"])?;
    anyhow::ensure!(
        managed_cloud_runner_headless_shell_path(relative_executable)
            && !revision.starts_with('0')
            && (1..=10).contains(&revision.len())
            && relative_executable.split('/').nth(1)
                == Some(
                    format!(
                        "chromium_headless_shell-{}",
                        d1_text(&browser["chromiumRevision"])?
                    )
                    .as_str()
                )
            && measurement
                .measured_files
                .iter()
                .filter(|item| managed_cloud_runner_headless_shell_path(&item.path))
                .count()
                == 1
            && measurement
                .measured_files
                .iter()
                .any(|item| item.path == relative_executable
                    && browser["executableSha256"] == item.sha256)
            && browser["titleReadback"] == true
            && browser["requestsAborted"].as_u64().is_some()
            && browser["launchProfile"] == "production_default_options_synthetic_only",
        "measured_d1_browser_binding"
    );
    let process_runtime = RunnerProcessRuntimeAttestation {
        runner_image_sha256: d1_digest(&inspection["oci"]["manifestSha256"])?.into(),
        // This is a declared deployment token, not D1's differently shaped buildId.
        runner_build_id: declared_runner_build_id.into(),
        platform: "linux".into(),
        architecture: "x86_64".into(),
        automation_bundle_sha256: bundle_sha256,
        playwright_version: d1_text(&browser["playwrightVersion"])?.into(),
        chromium_revision: d1_text(&browser["chromiumRevision"])?.into(),
        chromium_executable_sha256: d1_digest(&browser["executableSha256"])?.into(),
    };
    validate_runner_process_runtime(&process_runtime)?;
    validate_measured_d1_controller(input, admission, &manifest, &qualification)?;
    Ok(MeasuredRunnerD1Binding {
        source_commit: commit.into(),
        inventory_sha256: hashes[1].clone(),
        measurement_sha256: hashes[2].clone(),
        measurement,
        process_runtime,
        export_manifest_sha256: manifest_sha256,
        export_started_sha256: managed_cloud_sha256(&input.export_started),
        qualification_sha256,
        controller_journal_sha256: managed_cloud_sha256(&input.controller_journal),
        controller_final_stdout_sha256: managed_cloud_sha256(&input.controller_final_stdout),
        external_terminal_sha256: managed_cloud_sha256(&input.external_terminal),
    })
}

fn validate_startup_evidence_binding(
    encoded: &str,
    d1: &MeasuredRunnerD1Binding,
    release_test_evidence_sha256: &str,
) -> Result<String> {
    let bytes = managed_cloud_decode_base64url(encoded)?;
    let evidence = d1_json(&bytes, 128 * 1024, true)?;
    let expected = json!({"version":1,
        "audience":"bluey-jobs-phase641-nonproduction-startup-only-evidence-v1",
        "sourceCommit":d1.source_commit,"releaseTestEvidenceSha256":release_test_evidence_sha256,
        "d1ExportStartedSha256":d1.export_started_sha256,"d1ExportManifestSha256":d1.export_manifest_sha256,
        "d1QualificationSha256":d1.qualification_sha256,"d1ControllerJournalSha256":d1.controller_journal_sha256,
        "d1ControllerFinalStdoutSha256":d1.controller_final_stdout_sha256,"d1ExternalTerminalSha256":d1.external_terminal_sha256});
    anyhow::ensure!(evidence == expected, "measured_startup_evidence_aggregate");
    Ok(managed_cloud_sha256(&bytes))
}

fn validate_measured_startup_header(
    request: &MeasuredRunnerStartupRequest,
    d1: &MeasuredRunnerD1Binding,
) -> Result<()> {
    anyhow::ensure!(
        request.version == 1
            && request.operation == "issue_measured_runner_startup"
            && managed_cloud_route_id(&request.worker_id)
            && managed_cloud_route_id(&request.issuance_ref)
            && managed_cloud_route_id(&request.authorization_ref)
            && request.provider_resource_id.starts_with("phase641-owned-")
            && bounded_id(&request.provider_resource_id, 20, 128)
            && managed_cloud_hex64(&request.resource_fingerprint)
            && request.resource_fingerprint != "0".repeat(64)
            && (5_000..=900_000).contains(&request.ttl_ms),
        "measured_startup_request"
    );
    validate_runner_process_runtime(&request.process_runtime)?;
    anyhow::ensure!(
        request.process_runtime == d1.process_runtime,
        "measured_startup_process_binding"
    );
    Ok(())
}

fn validate_measured_startup_authority(
    request: &MeasuredRunnerStartupRequest,
    d1: &MeasuredRunnerD1Binding,
    anchor: &ManagedCloudRootTrustAnchor,
    now_ms: i64,
) -> Result<MeasuredRunnerAuthority> {
    validate_measured_startup_header(request, d1)?;
    let (policy_bytes, policy_signature_bytes) =
        managed_cloud_decode_envelope(&request.trust_policy)?;
    let policy: ManagedCloudTrustPolicyAuthority = managed_cloud_parse_canonical(&policy_bytes)?;
    let policy_signature: ManagedCloudSignatureSetAuthority =
        managed_cloud_parse_canonical(&policy_signature_bytes)?;
    validate_managed_cloud_root_anchor(anchor)?;
    validate_managed_cloud_trust_policy(&policy)?;
    anyhow::ensure!(
        policy.predecessor_policy_sha256.is_none(),
        "measured_startup_initial_policy"
    );
    verify_managed_cloud_bootstrap_union_signature_set(
        &policy_bytes,
        &policy_signature,
        anchor,
        &policy,
        now_ms,
    )?;

    let (release_bytes, release_signature_bytes) =
        managed_cloud_decode_envelope(&request.release.envelope)?;
    let release: ManagedCloudReleaseAuthority = managed_cloud_parse_canonical(&release_bytes)?;
    let release_signature: ManagedCloudSignatureSetAuthority =
        managed_cloud_parse_canonical(&release_signature_bytes)?;
    validate_managed_cloud_release(&release)?;
    anyhow::ensure!(
        release.version == 3
            && release.source_commit == d1.source_commit
            && release.manifest_generation == 1
            && release.release_sequence == 1,
        "measured_startup_release_version_source"
    );
    verify_managed_cloud_signature_set(
        &release_bytes,
        &release_signature,
        &policy,
        "release",
        &release.audience,
        release.published_at_ms,
        now_ms,
    )?;
    let verification_bytes =
        managed_cloud_decode_base64url(&request.release.verification_evidence_base64url)?;
    let verification: ManagedCloudVerificationEvidenceAuthority =
        managed_cloud_parse_canonical(&verification_bytes)?;
    validate_managed_cloud_verification_evidence(&verification, &verification_bytes, &release)?;
    let test_evidence_bytes =
        managed_cloud_decode_base64url(&request.release_test_evidence_base64url)?;
    let test_evidence = d1_json(&test_evidence_bytes, 128 * 1024, true)?;
    validate_startup_release_test_evidence(&test_evidence, &release.source_commit)?;
    anyhow::ensure!(
        managed_cloud_sha256(&test_evidence_bytes) == verification.test_evidence_sha256,
        "measured_startup_release_test_evidence_changed"
    );
    let startup_evidence_sha256 = validate_startup_evidence_binding(
        &request.startup_evidence_base64url,
        d1,
        &verification.test_evidence_sha256,
    )?;
    let failure_converter_sha256 = validate_managed_cloud_content_inventories(
        &request.release.inventory_attachments,
        &release,
        &verification,
    )?;
    let runner = release
        .components
        .iter()
        .find(|item| item.component_id == "jobs-runner")
        .context("measured startup runner component missing")?;
    let measured_runner = verification
        .components
        .iter()
        .find(|item| item.component_id == "jobs-runner")
        .context("measured startup runner verification missing")?;
    let inventory_bytes =
        managed_cloud_decode_content_inventory(&request.release.inventory_attachments.jobs_runner)?;
    anyhow::ensure!(
        runner.artifact_sha256 == d1.process_runtime.runner_image_sha256
            && runner.build_id == d1.measurement.build_id
            && managed_cloud_sha256(&inventory_bytes) == d1.inventory_sha256
            && measured_runner.runtime_measurement.as_ref() == Some(&d1.measurement)
            && measured_runner.runtime_measurement_sha256.as_deref()
                == Some(d1.measurement_sha256.as_str()),
        "measured_startup_d1_release_binding"
    );

    let (cohort_bytes, cohort_signature_bytes) =
        managed_cloud_decode_envelope(&request.cohort.envelope)?;
    let cohort: ManagedCloudCohortAuthority = managed_cloud_parse_canonical(&cohort_bytes)?;
    let cohort_signature: ManagedCloudSignatureSetAuthority =
        managed_cloud_parse_canonical(&cohort_signature_bytes)?;
    validate_managed_cloud_cohort(&cohort)?;
    anyhow::ensure!(
        cohort.scope == fixture_scope()
            && cohort.trust_generation == policy.trust_generation
            && request.cohort.account_ids.is_empty()
            && cohort.account_id_sha256s.is_empty()
            && cohort.cohort_generation == 1
            && cohort.not_before_ms >= policy.valid_from_ms
            && cohort.expires_at_ms <= policy.expires_at_ms
            && cohort.approval_ref
                == format!("phase641-startup-evidence-{startup_evidence_sha256}"),
        "measured_startup_cohort_scope"
    );
    verify_managed_cloud_signature_set(
        &cohort_bytes,
        &cohort_signature,
        &policy,
        "general_promotion",
        MANAGED_CLOUD_COHORT_AUDIENCE,
        cohort.issued_at_ms,
        now_ms,
    )?;

    let (activation_bytes, activation_signature_bytes) =
        managed_cloud_decode_envelope(&request.activation.envelope)?;
    let activation: ManagedCloudActivationAuthority =
        managed_cloud_parse_canonical(&activation_bytes)?;
    let activation_signature: ManagedCloudSignatureSetAuthority =
        managed_cloud_parse_canonical(&activation_signature_bytes)?;
    validate_managed_cloud_activation(&activation)?;
    let expires_at_ms = now_ms
        .checked_add(request.ttl_ms)
        .context("measured startup expiry overflow")?;
    anyhow::ensure!(
        activation.scope == fixture_scope()
            && activation.version == 3
            && activation.activation_generation == 1
            && activation.channel_sequence == 1
            && activation.expected_head_revision == 0
            && activation.expected_transition_sha256.is_none()
            && activation.predecessor_activation_sha256.is_none()
            && activation.recovery_acceptances.is_empty()
            && activation.manifest_sha256 == managed_cloud_sha256(&release_bytes)
            && activation.cohort_sha256 == managed_cloud_sha256(&cohort_bytes)
            && activation.trust_generation == policy.trust_generation
            && activation.not_before_ms <= now_ms
            && cohort.not_before_ms <= now_ms
            && activation.not_before_ms >= cohort.not_before_ms
            && activation.not_before_ms >= policy.valid_from_ms
            && activation.expires_at_ms <= cohort.expires_at_ms
            && activation.expires_at_ms <= policy.expires_at_ms
            && expires_at_ms <= activation.expires_at_ms
            && expires_at_ms <= cohort.expires_at_ms
            && expires_at_ms <= policy.expires_at_ms
            && activation.issued_at_ms >= release.published_at_ms
            && activation.issued_at_ms >= cohort.issued_at_ms,
        "measured_startup_activation_scope_time"
    );
    verify_managed_cloud_signature_set(
        &activation_bytes,
        &activation_signature,
        &policy,
        "general_promotion",
        &activation.audience,
        activation.issued_at_ms,
        now_ms,
    )?;
    let signature_ids: BTreeSet<_> = [
        &policy_signature,
        &release_signature,
        &cohort_signature,
        &activation_signature,
    ]
    .into_iter()
    .map(|signature| signature.signature_set_id.as_str())
    .collect();
    anyhow::ensure!(
        signature_ids.len() == 4,
        "measured_startup_signature_set_identity_conflict"
    );
    let dependencies = ManagedCloudActivationDependencies {
        trust_generation: policy.trust_generation,
        manifest_signature_set_sha256: managed_cloud_sha256(&release_signature_bytes),
        cohort_signature_set_sha256: managed_cloud_sha256(&cohort_signature_bytes),
        feature_authority_sha256: release.feature_authority_sha256.clone(),
        feature_authority: release.feature_authority,
        failure_converter_sha256,
        portal_artifact_sha256: release
            .components
            .iter()
            .find(|item| item.component_id == "jobs-portal")
            .context("measured startup portal component missing")?
            .artifact_sha256
            .clone(),
        manifest_published_at_ms: release.published_at_ms,
        cohort_issued_at_ms: cohort.issued_at_ms,
        cohort_not_before_ms: cohort.not_before_ms,
        cohort_expires_at_ms: cohort.expires_at_ms,
        canonical_manifest_base64url: request.release.envelope.canonical_base64url.clone(),
    };
    validate_managed_cloud_activation_dependencies(&activation, &dependencies)?;
    let evidence = parse_managed_cloud_activation_evidence(&request.activation.evidence)?;
    let validated_evidence = validate_managed_cloud_activation_evidence(
        &evidence,
        &activation,
        &dependencies.portal_artifact_sha256,
        &dependencies.failure_converter_sha256,
        now_ms,
    )?;
    validate_managed_cloud_activation_evidence_bindings(&activation, &validated_evidence)?;
    Ok(MeasuredRunnerAuthority {
        manifest_sha256: managed_cloud_sha256(&release_bytes),
        activation_sha256: managed_cloud_sha256(&activation_bytes),
        runtime_identity_sha256: managed_cloud_runtime_identity_sha256(
            &d1.measurement_sha256,
            "jobs-runner",
            "managed_runner",
        )?,
        expires_at_ms,
    })
}

const STARTUP_ZERO_TABLES: &[&str] = &[
    "jobs_managed_cloud_runtime_instances",
    "jobs_managed_cloud_runtime_heartbeats",
    "jobs_managed_cloud_original_source_verifier_runtime_instances",
    "jobs_managed_cloud_original_source_verifier_runtime_heartbeats",
    "jobs_runner_volumes",
    "jobs_runner_volume_keys",
    "jobs_runner_volume_authority_uses",
    "jobs_runner_process_runtime_bindings",
    "jobs_runner_volume_storage_attestations",
    "jobs_runner_legacy_inventory_authorities",
    "jobs_runner_volume_residencies",
    "jobs_runner_account_subjects",
    "jobs_runner_purge_requests",
    "jobs_runner_purge_tombstones",
    "jobs_runner_volume_destructions",
    "jobs_runner_process_runtime_grant_revocations",
    "jobs_applications",
    "jobs_browser_sessions",
    "jobs_attempt_reservations",
    "jobs_execution_leases",
    "jobs_workflow_commands",
    "jobs_managed_cloud_workflow_bindings",
    "jobs_managed_cloud_request_start_authorities",
    "jobs_managed_cloud_irreversible_effect_receipts",
];

fn require_startup_zero_state(pool: &DbPool) -> Result<()> {
    let mut connection = pool.get_pg()?;
    let mut tx = connection
        .build_transaction()
        .read_only(true)
        .isolation_level(postgres::IsolationLevel::RepeatableRead)
        .start()?;
    for table in STARTUP_ZERO_TABLES {
        let count: i64 = tx
            .query_one(&format!("SELECT count(*) FROM {table}"), &[])?
            .get(0);
        anyhow::ensure!(count == 0, "measured_startup_state_not_empty");
    }
    let fleet = tx.query_one(
        "SELECT enrollment_generation,purge_generation,tombstone_generation,destruction_generation,
                storage_attestation_generation,legacy_inventory_generation,cutover_state,
                legacy_reconciliation_generation,legacy_inventory_state,storage_attestation_count,
                unresolved_legacy_volume_count
           FROM jobs_runner_volume_fleet_state WHERE singleton_id=1",
        &[],
    )?;
    anyhow::ensure!(
        (0_usize..6).all(|index| fleet.get::<_, i64>(index) == 0)
            && fleet.get::<_, String>(6) == "pre_cutover"
            && fleet.get::<_, i64>(7) == 0
            && fleet.get::<_, String>(8) == "unknown"
            && fleet.get::<_, i64>(9) == 0
            && fleet.get::<_, i64>(10) == 0,
        "measured_startup_fleet_not_fresh"
    );
    tx.commit()?;
    Ok(())
}

fn require_startup_initial_registry(pool: &DbPool) -> Result<()> {
    let mut connection = pool.get_pg()?;
    for table in [
        "jobs_managed_cloud_trust_policies",
        "jobs_managed_cloud_manifests",
        "jobs_managed_cloud_cohorts",
        "jobs_managed_cloud_activations",
        "jobs_managed_cloud_heads",
        "jobs_managed_cloud_revocations",
        "jobs_managed_cloud_runtime_grants",
        "jobs_managed_cloud_original_source_verifier_runtime_grants",
        "jobs_runner_volume_admission_grants",
        "jobs_runner_process_runtime_grants",
    ] {
        let count: i64 = connection
            .query_one(&format!("SELECT count(*) FROM {table}"), &[])?
            .get(0);
        anyhow::ensure!(count == 0, "measured_startup_registry_not_fresh");
    }
    Ok(())
}

fn issue_startup_grants(
    pool: &DbPool,
    request: &MeasuredRunnerStartupRequest,
    authority: &MeasuredRunnerAuthority,
    now_ms: i64,
    d1: &MeasuredRunnerD1Binding,
) -> Result<MeasuredRunnerStartupResult> {
    let ttl_ms = authority
        .expires_at_ms
        .checked_sub(now_ms)
        .filter(|ttl| (5_000..=900_000).contains(ttl))
        .context("measured startup grant window expired")?;
    let created_by = "phase641-private-startup-issuer";
    let managed = issue_managed_cloud_runtime_grant(
        pool,
        &NewManagedCloudRuntimeGrant {
            issuance_ref: request.issuance_ref.clone(),
            scope: fixture_scope(),
            activation_sha256: authority.activation_sha256.clone(),
            manifest_sha256: authority.manifest_sha256.clone(),
            component_id: "jobs-runner".into(),
            role: "managed_runner".into(),
            expected_worker_id: request.worker_id.clone(),
            authorization_ref: request.authorization_ref.clone(),
            created_by: created_by.into(),
            ttl_ms,
        },
    )?;
    anyhow::ensure!(
        !managed.replayed
            && managed.grant_token.is_some()
            && managed.expected_runtime_identity_sha256 == authority.runtime_identity_sha256
            && managed.artifact_sha256 == request.process_runtime.runner_image_sha256,
        "measured_startup_managed_grant"
    );
    let volume_token = random_token();
    let volume = create_runner_volume_admission_grant(
        pool,
        &NewRunnerVolumeAdmissionGrant {
            grant_id: format!("p641-volume-{}", uuid::Uuid::new_v4().simple()),
            token: volume_token.clone(),
            expected_worker_id: request.worker_id.clone(),
            provider: "nonprod-owned-volume".into(),
            provider_resource_id: request.provider_resource_id.clone(),
            resource_fingerprint: request.resource_fingerprint.clone(),
            authorization_ref: request.authorization_ref.clone(),
            created_by: created_by.into(),
            expires_at_ms: authority.expires_at_ms,
            created_at_ms: now_ms,
        },
    )?;
    let process_token = random_token();
    let process = create_runner_process_runtime_grant(
        pool,
        &NewRunnerProcessRuntimeGrant {
            grant_id: format!("p641-process-{}", uuid::Uuid::new_v4().simple()),
            token: process_token.clone(),
            expected_worker_id: request.worker_id.clone(),
            runtime: request.process_runtime.clone(),
            authorization_ref: request.authorization_ref.clone(),
            created_by: created_by.into(),
            expires_at_ms: authority.expires_at_ms,
            created_at_ms: now_ms,
        },
    )?;
    anyhow::ensure!(
        volume.consumed_volume_id.is_none()
            && volume.consumed_at_ms.is_none()
            && volume.disposition == RunnerVolumeWriteDisposition::Applied
            && process.disposition == RunnerVolumeWriteDisposition::Applied,
        "measured_startup_grants_not_fresh"
    );
    Ok(MeasuredRunnerStartupResult {
        version: 1,
        operation: "issue_measured_runner_startup",
        worker_id: request.worker_id.clone(),
        scope: fixture_scope(),
        managed_runtime_grant: managed,
        volume_admission_grant: volume,
        volume_admission_token: volume_token,
        process_runtime_grant: process,
        process_runtime_token: process_token,
        d1_export_manifest_sha256: d1.export_manifest_sha256.clone(),
        d1_qualification_sha256: d1.qualification_sha256.clone(),
        d1_controller_journal_sha256: d1.controller_journal_sha256.clone(),
        grants_issued: 3,
        grants_consumed: 0,
        runtime_observed: false,
        customer_admission_proven: false,
        release_authority: false,
    })
}

/// Imports commit independently. Any later storage/clock failure retains partial
/// private state; a retry is refused by the initial-registry guard, not rolled back.
pub fn issue_measured_runner_startup(
    pool: &DbPool,
    request: &MeasuredRunnerStartupRequest,
    d1: &MeasuredRunnerD1Binding,
) -> Result<MeasuredRunnerStartupResult> {
    anyhow::ensure!(
        matches!(pool, DbPool::Postgres(_)),
        "measured startup requires private PostgreSQL"
    );
    require_startup_zero_state(pool)?;
    require_startup_initial_registry(pool)?;
    let anchor = managed_cloud_root_anchor_from_environment()?;
    let authority = validate_measured_startup_authority(request, d1, &anchor, clock(pool)?)?;
    let actor = "phase641-private-startup-issuer";
    import_managed_cloud_trust_policy(pool, &request.trust_policy, actor)?;
    let release = import_managed_cloud_release(pool, &request.release, actor)?;
    import_managed_cloud_cohort(pool, &request.cohort, actor)?;
    let activation = import_managed_cloud_activation(pool, &request.activation, actor)?;
    anyhow::ensure!(
        release.authority_sha256 == authority.manifest_sha256
            && activation.authority_sha256 == authority.activation_sha256,
        "measured_startup_import_binding"
    );
    apply_managed_cloud_activation(
        pool,
        &ApplyManagedCloudActivationRequest {
            activation_sha256: authority.activation_sha256.clone(),
            expected_head_revision: 0,
            expected_transition_sha256: None,
        },
        actor,
    )?;
    require_startup_zero_state(pool)?;
    let result = issue_startup_grants(pool, request, &authority, clock(pool)?, d1)?;
    require_startup_zero_state(pool)?;
    let counts = pool.get_pg()?.query_one(
        "SELECT (SELECT count(*) FROM jobs_managed_cloud_runtime_grants),
                (SELECT count(*) FROM jobs_runner_volume_admission_grants),
                (SELECT count(*) FROM jobs_runner_process_runtime_grants)",
        &[],
    )?;
    anyhow::ensure!(
        (0_usize..3).all(|index| counts.get::<_, i64>(index) == 1),
        "measured_startup_three_grants"
    );
    Ok(result)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FixtureMode {
    ReviewFirst,
    AutoSubmit,
}

impl FixtureMode {
    fn from_submission_mode(value: &str) -> Result<Self> {
        match value {
            "review_first" => Ok(Self::ReviewFirst),
            "auto_submit" => Ok(Self::AutoSubmit),
            _ => anyhow::bail!("fixture checkpoint submission mode"),
        }
    }

    fn bootstrap_operation(self) -> &'static str {
        match self {
            Self::ReviewFirst => "bootstrap",
            Self::AutoSubmit => "bootstrap_auto_submit",
        }
    }

    fn post_queue_operation(self) -> &'static str {
        match self {
            Self::ReviewFirst => "post_queue",
            Self::AutoSubmit => "post_queue_auto_submit",
        }
    }

    fn submission_mode(self) -> &'static str {
        match self {
            Self::ReviewFirst => "review_first",
            Self::AutoSubmit => "auto_submit",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneBootstrapRequest {
    pub version: u8,
    pub operation: String,
    pub account_id: String,
    pub track_id: String,
    pub worker_id: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneVolume {
    pub volume_id: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub key_fingerprint: String,
    pub runtime_grant_id: String,
    pub runtime_sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlanePrivateVolumeKey {
    pub version: u8,
    pub worker_id: String,
    pub volume: RunnerControlPlaneVolume,
    pub public_key_base64url: String,
    pub signing_seed_base64url: String,
}

// Match the SDK's closed camelCase claim descriptor without changing the production type.
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "ManagedCloudRuntimeInstance",
    rename_all = "camelCase",
    deny_unknown_fields
)]
struct RuntimeInstanceWire {
    grant_id: String,
    runtime_instance_id: String,
    runtime_identity_sha256: String,
    worker_id: String,
    scope: ManagedCloudScope,
    activation_sha256: String,
    manifest_sha256: String,
    component_id: String,
    role: String,
    head_revision: i64,
    transition_sha256: String,
    artifact_sha256: String,
    config_schema_sha256: String,
    migration_set_sha256: String,
    protocol_set_sha256: String,
    task_queue_sha256: String,
    failure_converter_sha256: String,
    dependency_evidence_sha256: String,
    activation_expires_at_ms: i64,
    instance_epoch: i64,
    next_heartbeat_sequence: i64,
    claimed_at_ms: i64,
    replayed: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneRuntimeCredentials {
    #[serde(with = "RuntimeInstanceWire")]
    pub instance: ManagedCloudRuntimeInstance,
    pub session_token: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneBootstrapResult {
    pub version: u8,
    pub operation: String,
    pub account_id: String,
    pub track_id: String,
    pub job_id: String,
    pub managed_release_fixture_version: u8,
    pub scope: ManagedCloudScope,
    pub volume: RunnerControlPlaneVolume,
    pub runtimes: Vec<RunnerControlPlaneRuntimeCredentials>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlanePostQueueRequest {
    pub version: u8,
    pub operation: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub command_id: String,
    pub request_id: String,
    pub workflow_id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlanePostQueueResult {
    pub version: u8,
    pub operation: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub command_id: String,
    pub request_id: String,
    pub workflow_id: String,
    pub submission_mode: String,
    pub managed_cloud_release: ManagedCloudReleaseMemoAuthority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_summary: Option<RunnerControlPlaneAutoSummary>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneAutoSummary {
    pub attempt_id: String,
    pub auto_authorization_id: String,
    pub auto_authorization_revision: i64,
    pub auto_authorization_fingerprint_sha256: String,
    pub packet_checksum_sha256: String,
    pub provider: String,
    pub target_key_sha256: String,
    pub manifest_sha256: String,
    pub activation_sha256: String,
    pub layout_set_sha256: String,
    pub adapter_bundle_sha256: String,
    pub runner_target_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneInspectAutoSubmitClaimRequest {
    pub version: u8,
    pub operation: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub lease_token_sha256: String,
    pub lease_fence: i64,
}

#[derive(Serialize)]
pub struct RunnerControlPlaneInspectAutoSubmitClaimResult {
    pub version: u8,
    pub operation: &'static str,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub lease_token_sha256: String,
    pub lease_fence: i64,
    pub ats_account_count: i64,
    pub ats_application_count: i64,
    pub ats_run_count: i64,
    pub binding_sha256: String,
    pub nonce_sha256: String,
    pub phase: String,
    pub binding_fence: i64,
    pub expires_future: bool,
    pub exact_frozen_authority: bool,
    pub exact_prepared_lease: bool,
    pub no_phase_b_or_invalidation: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunnerControlPlaneCheckpointStage {
    BeforeRecovery,
    AfterRecovery,
    AfterExactReplay,
}

impl RunnerControlPlaneCheckpointStage {
    fn is_before(self) -> bool {
        self == Self::BeforeRecovery
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneInspectCheckpointRecoveryRequest {
    pub version: u8,
    pub operation: String,
    pub stage: RunnerControlPlaneCheckpointStage,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneCloudRecoverySummary {
    pub schema_version: i64,
    pub status: String,
    pub checkpoint_version: i64,
    pub checkpoint_phase: String,
    pub lease_phase: String,
    pub recorded_at_ms: i64,
    pub run_id: String,
}

#[derive(Serialize)]
pub struct RunnerControlPlaneInspectCheckpointRecoveryResult {
    pub version: u8,
    pub operation: &'static str,
    pub stage: RunnerControlPlaneCheckpointStage,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub submission_mode: String,
    pub application_stable_sha256: String,
    pub session_stable_sha256: String,
    pub application_state: String,
    pub session_status: String,
    pub session_current_step: String,
    pub submitted_absent: bool,
    pub takeover_absent: bool,
    pub lease_phase: String,
    pub attempt_status: String,
    pub cloud_recovery: Option<RunnerControlPlaneCloudRecoverySummary>,
    pub cloud_recovery_sha256: Option<String>,
}

fn fixture_scope() -> ManagedCloudScope {
    ManagedCloudScope {
        environment: "staging".into(),
        region: "us-east-1".into(),
        channel: "general".into(),
    }
}

fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn bounded_id(value: &str, minimum: usize, maximum: usize) -> bool {
    (minimum..=maximum).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn clock(pool: &DbPool) -> Result<i64> {
    Ok(pool
        .get_pg()?
        .query_one(
            "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint",
            &[],
        )?
        .get(0))
}

fn synthetic_account_id(value: &str) -> bool {
    value.strip_prefix("ui_fixture_").is_some_and(|suffix| {
        uuid::Uuid::parse_str(suffix)
            .is_ok_and(|id| id.get_version_num() == 4 && id.to_string() == suffix)
    })
}

fn runtime_model(label: &str) -> RunnerProcessRuntimeAttestation {
    RunnerProcessRuntimeAttestation {
        runner_image_sha256: digest(&format!("synthetic-image:{label}")),
        // Canonical protocol build token, not a claim that this build/image was executed.
        runner_build_id: "runner-638.0".into(),
        platform: "linux".into(),
        architecture: "x86_64".into(),
        automation_bundle_sha256: digest(&format!("synthetic-automation:{label}")),
        playwright_version: "1.61.1".into(),
        chromium_revision: "1228".into(),
        chromium_executable_sha256: digest(&format!("synthetic-chromium:{label}")),
    }
}

fn runtime_target(
    runtime: &RunnerProcessRuntimeAttestation,
) -> Result<AtsCertificationRuntimeTarget> {
    Ok(AtsCertificationRuntimeTarget {
        runtime_kind: "cloud".into(),
        runtime_id: format!("cloud:{}", runtime.runner_build_id),
        runtime_sha256: runner_process_runtime_sha256(runtime)?,
        platform: runtime.platform.clone(),
        architecture: runtime.architecture.clone(),
        automation_bundle_sha256: runtime.automation_bundle_sha256.clone(),
        browser_release_manifest_sha256: None,
        browser_artifact_sha256: None,
        browser_build_descriptor_sha256: None,
        runner_build_id: Some(runtime.runner_build_id.clone()),
        runner_image_sha256: Some(runtime.runner_image_sha256.clone()),
        playwright_version: runtime.playwright_version.clone(),
        chromium_revision: runtime.chromium_revision.clone(),
        chromium_executable_sha256: runtime.chromium_executable_sha256.clone(),
    })
}

pub fn bootstrap(
    pool: &DbPool,
    request: &RunnerControlPlaneBootstrapRequest,
    volume_seed: [u8; 32],
) -> Result<(
    RunnerControlPlaneBootstrapResult,
    RunnerControlPlanePrivateVolumeKey,
)> {
    bootstrap_with_mode(pool, request, volume_seed, FixtureMode::ReviewFirst)
}

pub fn bootstrap_auto_submit(
    pool: &DbPool,
    request: &RunnerControlPlaneBootstrapRequest,
    volume_seed: [u8; 32],
) -> Result<(
    RunnerControlPlaneBootstrapResult,
    RunnerControlPlanePrivateVolumeKey,
)> {
    bootstrap_with_mode(pool, request, volume_seed, FixtureMode::AutoSubmit)
}

fn bootstrap_with_mode(
    pool: &DbPool,
    request: &RunnerControlPlaneBootstrapRequest,
    volume_seed: [u8; 32],
    mode: FixtureMode,
) -> Result<(
    RunnerControlPlaneBootstrapResult,
    RunnerControlPlanePrivateVolumeKey,
)> {
    anyhow::ensure!(
        matches!(pool, DbPool::Postgres(_)),
        "fixture requires private PostgreSQL"
    );
    anyhow::ensure!(
        request.version == 1 && request.operation == mode.bootstrap_operation(),
        "fixture request version"
    );
    anyhow::ensure!(
        synthetic_account_id(&request.account_id)
            && bounded_id(&request.track_id, 1, 120)
            && bounded_id(&request.worker_id, 20, 128),
        "fixture identifiers"
    );
    let account = crate::db::accounts::Account::fetch_by_id(pool, &request.account_id)?
        .context("fixture account missing")?;
    anyhow::ensure!(
        account.email_verified_at.is_some() && account.email.ends_with("@example.test"),
        "fixture requires synthetic verified account"
    );
    let saved = pool.get_pg()?.query_one(
        "SELECT EXISTS(SELECT 1 FROM jobs_profiles WHERE account_id=$1),
         EXISTS(SELECT 1 FROM jobs_preferences WHERE account_id=$1)",
        &[&account.id],
    )?;
    anyhow::ensure!(
        saved.get::<_, bool>(0) && saved.get::<_, bool>(1),
        "fixture requires saved candidate"
    );
    let profile = get_profile(pool, &account.id, &account.email)?;
    let preferences = get_preferences(pool, &account.id)?;
    anyhow::ensure!(
        profile.onboarding_complete,
        "fixture requires saved onboarding"
    );
    let track = list_tracks(pool, &account.id)?
        .into_iter()
        .find(|track| track.id == request.track_id && track.active)
        .context("fixture track missing")?;
    let identity_id = track
        .application_identity_id
        .as_ref()
        .context("fixture track identity missing")?;
    anyhow::ensure!(
        list_application_identities(pool, &account.id)?
            .iter()
            .any(|identity| identity.id == *identity_id
                && identity.verification_status == "verified"),
        "fixture track requires verified identity"
    );
    anyhow::ensure!(
        list_applications(pool, &account.id)?.is_empty()
            && list_browser_sessions(pool, &account.id)?.is_empty()
            && list_attempt_reservations(pool, &account.id)?.is_empty(),
        "fixture account is not fresh"
    );
    let fleet = runner_volume_fleet_status(pool)?;
    anyhow::ensure!(
        fleet.non_destroyed_volume_count == 0
            && fleet.destruction_count == 0
            && fleet.unresolved_legacy_volume_count == 0,
        "fixture fleet is not fresh"
    );
    let label = uuid::Uuid::new_v4().simple().to_string();
    let runtime = runtime_model(&label);
    let target = runtime_target(&runtime)?;
    validate_ats_runtime_target(&target)?;
    let plan = set_entitlement_plan(pool, &account.id, "cloud")
        .context("fixture_bootstrap_entitlement")?;
    anyhow::ensure!(
        plan.cloud_browser && plan.used_packets == 0,
        "fixture cloud entitlement"
    );
    let now = clock(pool)?;
    let mut posting = JobPosting {
        id: String::new(), canonical_key: String::new(), source: "greenhouse".into(),
        external_id: format!("phase638-{label}"), company: "Synthetic Protocol Employer".into(),
        title: "Software Engineer".into(), location: "New York, NY".into(), workplace: "hybrid".into(),
        canonical_url: format!("https://boards.greenhouse.io/blueynonprod/jobs/phase638-{label}"),
        description: "Full-time W2 employment. Hybrid work in New York, NY. Visa sponsorship available. Build reliable software systems with Rust and TypeScript.".into(),
        compensation: "$170k-$200k".into(), employment_type: "full_time w2".into(),
        track_id: track.id, match_score: 99, matched_reasons: Vec::new(), missing_requirements: Vec::new(),
        posted_at_ms: Some(now), last_verified_at_ms: Some(now), availability_status: "active".into(),
        status: "matched".into(), created_at_ms: 0, updated_at_ms: 0,
        discovery_evidence: JobDiscoveryEvidence::default(), eligibility: None,
    };
    if mode == FixtureMode::AutoSubmit {
        // Auto-only synthetic facts must match the caller's uploaded source/profile.
        // This adds the third skill needed to meet the unchanged default score of 80.
        posting.description = posting
            .description
            .replace("Rust and TypeScript", "Rust, TypeScript, and PostgreSQL");
    }
    posting.canonical_key = canonical_job_key(&posting);
    // Existing source-verifier fixture retains its historical synthetic production scope ONLY
    // inside the physically guarded private DB. New runner/admission scope is always staging.
    let (posting, source_runtime) =
        production_positive_authority_fixture::save_production_positive_verified_import(
            pool,
            &account.id,
            &posting,
            &profile,
            &preferences,
        );
    let positive = production_positive_authority_fixture::install_production_positive_job_authorities_with_runtime_surface(
        pool, &account.id, &posting, &source_runtime, "phase638-employer.example.test",
        &format!("phase638-{label}"), target,
        AtsObservedSurface { variant_key: "greenhouse_public".into(), layout_contract_version: 1,
            surface_sha256: digest(&format!("synthetic-surface:{label}")) },
    );
    anyhow::ensure!(
        positive.composed.job_integrity.status == JobIntegrityResolutionStatus::Verified,
        "fixture signed job authority unavailable"
    );
    anyhow::ensure!(
        evaluate_job_eligibility(pool, &account.id, &posting, false, None)
            .context("fixture_bootstrap_eligibility")?
            .can_prepare,
        "fixture signed posting not preparable"
    );
    if mode == FixtureMode::AutoSubmit {
        let eligibility = evaluate_job_eligibility(pool, &account.id, &posting, false, None)
            .context("fixture_bootstrap_auto_eligibility")?;
        anyhow::ensure!(
            eligibility.can_auto_submit
                && eligibility.can_queue_cloud
                && eligibility.hard_failures.is_empty()
                && eligibility.review_reasons.is_empty()
                && posting.missing_requirements.is_empty()
                && posting.match_score >= profile.auto_submit_threshold,
            "fixture auto-submit posting not eligible"
        );
    }
    let scope = fixture_scope();
    let managed =
        managed_cloud_release_authority_tests::install_managed_runner_runtime_test_fixture(
            pool,
            &request.worker_id,
            scope.clone(),
            source_runtime.now_ms,
        );
    let runtimes: Vec<_> = managed
        .runtime_credentials
        .into_iter()
        .map(
            |(instance, session_token)| RunnerControlPlaneRuntimeCredentials {
                instance,
                session_token,
            },
        )
        .collect();
    validate_runtime_set(&runtimes, &request.worker_id)
        .context("fixture_bootstrap_runtime_descriptors")?;
    let (volume, private_key) =
        provision_volume(pool, &request.worker_id, volume_seed, runtime, &label)
            .context("fixture_bootstrap_volume")?;
    let readiness = resolve_managed_cloud_readiness(
        pool,
        &ManagedCloudReadinessQuery {
            scope: scope.clone(),
            account_id: Some(account.id.clone()),
        },
    )
    .context("fixture_bootstrap_readiness")?;
    anyhow::ensure!(
        readiness.status.customer_admission
            && readiness.missing_roles.is_empty()
            && readiness.stale_roles.is_empty(),
        "fixture managed readiness unavailable"
    );
    Ok((
        RunnerControlPlaneBootstrapResult {
            version: 1,
            operation: mode.bootstrap_operation().into(),
            account_id: account.id,
            track_id: request.track_id.clone(),
            job_id: posting.id,
            managed_release_fixture_version: 2,
            scope,
            volume,
            runtimes,
        },
        private_key,
    ))
}

fn validate_runtime_set(
    runtimes: &[RunnerControlPlaneRuntimeCredentials],
    worker_id: &str,
) -> Result<()> {
    let expected: BTreeSet<_> = [
        "jobs_api",
        "managed_runner",
        "original_source_verifier",
        "workflow_cleanup_dispatcher",
        "workflow_command_dispatcher",
        "workflow_gateway",
        "workflow_worker",
    ]
    .into_iter()
    .collect();
    let actual: BTreeSet<_> = runtimes
        .iter()
        .map(|entry| entry.instance.role.as_str())
        .collect();
    anyhow::ensure!(
        runtimes.len() == 7 && actual == expected,
        "fixture requires exactly seven runtime roles"
    );
    for entry in runtimes {
        anyhow::ensure!(
            entry.instance.scope == fixture_scope()
                && entry.instance.next_heartbeat_sequence >= 2
                && decode_base64url_exact(&entry.session_token, 32).is_ok(),
            "fixture runtime descriptor"
        );
        if entry.instance.role == "managed_runner" {
            anyhow::ensure!(
                entry.instance.worker_id == worker_id,
                "fixture managed worker binding"
            );
        }
    }
    Ok(())
}

fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    encode_base64url(&bytes)
}

fn provision_volume(
    pool: &DbPool,
    worker_id: &str,
    seed: [u8; 32],
    runtime: RunnerProcessRuntimeAttestation,
    label: &str,
) -> Result<(RunnerControlPlaneVolume, RunnerControlPlanePrivateVolumeKey)> {
    let signing_key = Ed25519SigningKey::from_bytes(&seed);
    let public_key_base64url = encode_base64url(signing_key.verifying_key().as_bytes());
    let volume_id = runner_volume_id_from_public_key(&public_key_base64url)?;
    let key_fingerprint = runner_volume_key_fingerprint(&public_key_base64url)?;
    let process_instance_id = random_token();
    let provider_resource_id = format!("synthetic-protocol-resource-{label}");
    let resource_fingerprint = digest(&provider_resource_id);
    let token = random_token();
    let now = clock(pool)?;
    let expires_at_ms = now.checked_add(900_000).context("fixture clock overflow")?;
    let grant = NewRunnerVolumeAdmissionGrant {
        grant_id: format!("volume-grant-{label}"),
        token: token.clone(),
        expected_worker_id: worker_id.into(),
        provider: "test-provider".into(),
        provider_resource_id: provider_resource_id.clone(),
        resource_fingerprint: resource_fingerprint.clone(),
        authorization_ref: format!("synthetic-protocol-{label}"),
        created_by: "phase638-protocol-fixture".into(),
        expires_at_ms,
        created_at_ms: now,
    };
    create_runner_volume_admission_grant(pool, &grant).context("fixture_volume_admission")?;
    let proof = sign_runner_volume_enrollment_proof(
        &signing_key,
        NewRunnerVolumeEnrollmentProof {
            admission_grant_id: grant.grant_id,
            volume_id: volume_id.clone(),
            worker_id: worker_id.into(),
            provider: grant.provider,
            provider_resource_id,
            resource_fingerprint: resource_fingerprint.clone(),
            enrollment_epoch: 1,
            public_key_base64url: public_key_base64url.clone(),
            key_fingerprint: key_fingerprint.clone(),
            legacy_artifact_count: 0,
            requested_at_ms: now,
        },
    )?;
    enroll_runner_volume(
        pool,
        &EnrollRunnerVolumeRequest {
            grant_token: token,
            proof,
            enrolled_at_ms: now,
        },
    )
    .context("fixture_volume_enrollment")?;
    let runtime_grant_id = format!("runtime-grant-{label}");
    let runtime_token = random_token();
    let runtime_sha256 = runner_process_runtime_sha256(&runtime)?;
    let runtime_grant = create_runner_process_runtime_grant(
        pool,
        &NewRunnerProcessRuntimeGrant {
            grant_id: runtime_grant_id.clone(),
            token: runtime_token.clone(),
            expected_worker_id: worker_id.into(),
            runtime: runtime.clone(),
            authorization_ref: format!("synthetic-runtime-{label}"),
            created_by: "phase638-protocol-fixture".into(),
            expires_at_ms,
            created_at_ms: now,
        },
    )
    .context("fixture_volume_runtime_grant")?;
    anyhow::ensure!(
        runtime_grant.runtime_sha256 == runtime_sha256,
        "fixture runtime grant binding"
    );
    let path = format!("/api/jobs/internal/runner-volumes/{volume_id}/instances/claim");
    let token_sha256 = digest(&runtime_token);
    let payload = crate::api::jobs_runner_volumes::runner_volume_http_payload_sha256(
        &path,
        worker_id,
        &[
            ("runtime_grant_id", runtime_grant_id.as_str()),
            ("runtime_grant_token_sha256", token_sha256.as_str()),
            ("runtime_sha256", runtime_sha256.as_str()),
        ],
    );
    let proof = sign_runner_volume_authority_proof(
        &signing_key,
        NewRunnerVolumeAuthorityProof {
            operation: "instance_claim".into(),
            request_id: format!("instance-claim-{label}"),
            volume_id: volume_id.clone(),
            enrollment_epoch: 1,
            process_instance_id: process_instance_id.clone(),
            issued_at_ms: now,
            payload_sha256: payload.clone(),
        },
    )?;
    let authority = verify_runner_volume_authority_proof(
        pool,
        &proof,
        "instance_claim",
        &payload,
        now,
        90_000,
    )?;
    let claimed = claim_runner_volume_instance_authorized(
        pool,
        &RunnerVolumeInstanceLeaseRequest {
            volume_id: &volume_id,
            enrollment_epoch: 1,
            process_instance_id: &process_instance_id,
            now_ms: now,
            lease_expires_at_ms: expires_at_ms,
        },
        &RunnerProcessRuntimeGrantClaim {
            grant_id: runtime_grant_id.clone(),
            grant_token: runtime_token,
            runtime: runtime.clone(),
        },
        &authority,
    )
    .context("fixture_volume_instance_claim")?;
    anyhow::ensure!(
        claimed.runtime_grant_id.as_deref() == Some(runtime_grant_id.as_str())
            && claimed.runtime_sha256.as_deref() == Some(runtime_sha256.as_str()),
        "fixture runtime claim binding"
    );
    let current =
        lookup_runner_volume(pool, &volume_id)?.context("fixture enrolled volume missing")?;
    let predecessor = runner_volume_storage_attestation_predecessor(
        pool,
        &volume_id,
        1,
        &process_instance_id,
        now,
    )?;
    // Empty synthetic storage authority, not evidence from a filesystem or a VM.
    let attestation = sign_runner_volume_storage_attestation(
        &signing_key,
        NewRunnerVolumeStorageAttestation {
            attestation_id: format!("synthetic-storage-{label}"),
            volume_id: volume_id.clone(),
            volume_key_fingerprint: key_fingerprint.clone(),
            resource_fingerprint,
            enrollment_epoch: 1,
            enrollment_generation: current.enrollment_generation,
            process_instance_id: process_instance_id.clone(),
            predecessor_attestation_generation: predecessor.predecessor_attestation_generation,
            predecessor_attestation_sha256: predecessor.predecessor_attestation_sha256,
            required_tombstone_generation: current.required_tombstone_generation,
            reconciled_tombstone_generation: current.reconciled_tombstone_generation,
            storage_evidence_version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
            subject_storage_layout_version: RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION,
            root_device_id: "unix:638:synthetic-empty-root".into(),
            root_link_count: 7,
            root_entry_count: 0,
            root_file_bytes: "0".into(),
            root_sha256: EMPTY_RUNNER_INVENTORY_SHA256.into(),
            subject_storage_subject_count: 0,
            subject_storage_subject_set_sha256: EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256
                .into(),
            subject_storage_scope_count: 0,
            subject_storage_complete_root_entry_count: 0,
            subject_storage_complete_root_file_bytes: "0".into(),
            subject_storage_complete_root_sha256: EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256
                .into(),
            locator_count: 0,
            resident_locator_count: 0,
            locator_set_sha256: EMPTY_RUNNER_INVENTORY_SHA256.into(),
            legacy_inventory_version: RUNNER_LEGACY_INVENTORY_VERSION,
            legacy_artifact_count: 0,
            legacy_artifact_bytes: "0".into(),
            legacy_artifact_set_sha256: EMPTY_RUNNER_LEGACY_ARTIFACT_SET_SHA256.into(),
            unclassified_root_count: 0,
            runner_build_id: runtime.runner_build_id.clone(),
            observed_at_ms: now,
        },
    )
    .context("fixture_volume_sign_attestation")?;
    let payload = attestation.attestation_sha256()?;
    let proof = sign_runner_volume_authority_proof(
        &signing_key,
        NewRunnerVolumeAuthorityProof {
            operation: "storage_attestation".into(),
            request_id: format!("storage-proof-{label}"),
            volume_id: volume_id.clone(),
            enrollment_epoch: 1,
            process_instance_id: process_instance_id.clone(),
            issued_at_ms: now,
            payload_sha256: payload.clone(),
        },
    )?;
    let authority = verify_runner_volume_authority_proof(
        pool,
        &proof,
        "storage_attestation",
        &payload,
        now,
        90_000,
    )?;
    record_runner_volume_storage_attestation(
        pool,
        &attestation,
        &runtime.runner_build_id,
        now,
        &authority,
    )
    .context("fixture_volume_record_attestation")?;
    activate_reconciled_runner_volume(pool, &volume_id, 1, &process_instance_id, now)
        .context("fixture_volume_activation")?;
    prepare_fleet(pool, label, clock(pool)?).context("fixture_volume_fleet")?;
    let volume = RunnerControlPlaneVolume {
        volume_id,
        enrollment_epoch: 1,
        process_instance_id,
        key_fingerprint,
        runtime_grant_id,
        runtime_sha256,
    };
    let private = RunnerControlPlanePrivateVolumeKey {
        version: 1,
        worker_id: worker_id.into(),
        volume: volume.clone(),
        public_key_base64url,
        signing_seed_base64url: encode_base64url(&seed),
    };
    Ok((volume, private))
}

fn prepare_fleet(pool: &DbPool, label: &str, now: i64) -> Result<()> {
    let status = runner_volume_fleet_status(pool).context("fixture_fleet_initial_status")?;
    anyhow::ensure!(
        status.legacy_inventory_state == "unknown",
        "fixture legacy inventory already initialized"
    );
    let reconciling = record_runner_legacy_inventory_authority(
        pool,
        &RecordRunnerLegacyInventoryAuthorityRequest {
            reconciliation_id: format!("synthetic-inventory-{label}"),
            authority_state: "reconciling".into(),
            expected_predecessor_generation: status.legacy_inventory_generation,
            expected_predecessor_authority_id: status.legacy_inventory_authority_id,
            expected_predecessor_authority_sha256: status.legacy_inventory_authority_sha256,
            root_count: 0,
            root_set_sha256: EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256.into(),
            scope_ref: "all-managed-runner-storage-roots".into(),
            evidence_ref: format!("synthetic-inventory-start-{label}"),
            evidence_sha256: digest(&format!("synthetic-inventory-start:{label}")),
            authorized_by: "phase638-protocol-fixture".into(),
            recorded_at_ms: now,
        },
    )
    .context("fixture_fleet_inventory_begin")?
    .authority;
    record_runner_legacy_inventory_authority(
        pool,
        &RecordRunnerLegacyInventoryAuthorityRequest {
            reconciliation_id: reconciling.reconciliation_id,
            authority_state: "ready".into(),
            expected_predecessor_generation: reconciling.authority_generation,
            expected_predecessor_authority_id: Some(reconciling.authority_id),
            expected_predecessor_authority_sha256: Some(reconciling.authority_sha256),
            root_count: reconciling.root_count,
            root_set_sha256: reconciling.root_set_sha256,
            scope_ref: reconciling.scope_ref,
            evidence_ref: format!("synthetic-inventory-ready-{label}"),
            evidence_sha256: digest(&format!("synthetic-inventory-ready:{label}")),
            authorized_by: "phase638-protocol-fixture".into(),
            recorded_at_ms: now,
        },
    )
    .context("fixture_fleet_inventory_ready")?;
    let status = runner_volume_fleet_status(pool).context("fixture_fleet_inventory_status")?;
    let mut request = RecordRunnerVolumeFleetCutoverRequest {
        cutover_state: "reconciling".into(),
        expected_enrollment_generation: status.enrollment_generation,
        expected_purge_generation: status.purge_generation,
        expected_tombstone_generation: status.tombstone_generation,
        expected_destruction_generation: status.destruction_generation,
        expected_legacy_reconciliation_generation: status.legacy_reconciliation_generation,
        expected_storage_attestation_generation: status.storage_attestation_generation,
        expected_storage_attestation_count: status.storage_attestation_count,
        expected_storage_attestation_set_sha256: status.storage_attestation_set_sha256,
        expected_legacy_inventory_generation: status.legacy_inventory_generation,
        expected_legacy_inventory_reconciliation_id: status
            .legacy_inventory_reconciliation_id
            .context("fixture inventory missing")?,
        expected_legacy_inventory_authority_id: status
            .legacy_inventory_authority_id
            .context("fixture inventory missing")?,
        expected_legacy_inventory_authority_sha256: status
            .legacy_inventory_authority_sha256
            .context("fixture inventory missing")?,
        expected_legacy_inventory_root_count: status
            .legacy_inventory_root_count
            .context("fixture inventory missing")?,
        expected_legacy_inventory_root_set_sha256: status
            .legacy_inventory_root_set_sha256
            .context("fixture inventory missing")?,
        expected_non_destroyed_volume_count: status.non_destroyed_volume_count,
        expected_destruction_count: status.destruction_count,
        expected_unresolved_legacy_volume_count: status.unresolved_legacy_volume_count,
        evidence_ref: format!("synthetic-cutover-{label}"),
        evidence_sha256: digest(&format!("synthetic-cutover:{label}")),
        authorized_by: "phase638-protocol-fixture".into(),
        cutover_at_ms: now,
        now_ms: now,
    };
    record_runner_volume_fleet_cutover(pool, &request).context("fixture_fleet_cutover_begin")?;
    request.cutover_state = "ready".into();
    record_runner_volume_fleet_cutover(pool, &request).context("fixture_fleet_cutover_ready")?;
    let final_status = runner_volume_fleet_status(pool).context("fixture_fleet_final_status")?;
    anyhow::ensure!(
        final_status.cutover_state == "ready"
            && final_status.legacy_inventory_state == "ready"
            && final_status.non_destroyed_volume_count == 1
            && final_status.attested_reconciled_volume_count == 1,
        "fixture fleet not ready"
    );
    Ok(())
}

// No command claim, request-start publication, session alias, residency or lease creation.
// PostgreSQL itself enforces read-only operation for this entire handoff snapshot.
pub fn post_queue(
    pool: &DbPool,
    request: &RunnerControlPlanePostQueueRequest,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
) -> Result<RunnerControlPlanePostQueueResult> {
    post_queue_with_mode(
        pool,
        request,
        bootstrap,
        private_key,
        FixtureMode::ReviewFirst,
    )
}

pub fn post_queue_auto_submit(
    pool: &DbPool,
    request: &RunnerControlPlanePostQueueRequest,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
) -> Result<RunnerControlPlanePostQueueResult> {
    post_queue_with_mode(
        pool,
        request,
        bootstrap,
        private_key,
        FixtureMode::AutoSubmit,
    )
}

fn validate_bootstrap_binding(
    mode: FixtureMode,
    account_id: &str,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
) -> Result<()> {
    anyhow::ensure!(
        bootstrap.version == 1
            && bootstrap.operation == mode.bootstrap_operation()
            && bootstrap.managed_release_fixture_version == 2
            && bootstrap.scope == fixture_scope()
            && bootstrap.account_id == account_id
            && synthetic_account_id(account_id)
            && private_key.version == 1
            && private_key.volume == bootstrap.volume,
        "fixture bootstrap binding"
    );
    validate_runtime_set(&bootstrap.runtimes, &private_key.worker_id)?;
    let seed: [u8; 32] = decode_base64url_exact(&private_key.signing_seed_base64url, 32)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("fixture private key length"))?;
    let signing_key = Ed25519SigningKey::from_bytes(&seed);
    anyhow::ensure!(
        encode_base64url(signing_key.verifying_key().as_bytes())
            == private_key.public_key_base64url
            && runner_volume_id_from_public_key(&private_key.public_key_base64url)?
                == bootstrap.volume.volume_id
            && runner_volume_key_fingerprint(&private_key.public_key_base64url)?
                == bootstrap.volume.key_fingerprint,
        "fixture private volume binding"
    );
    Ok(())
}

fn post_queue_with_mode(
    pool: &DbPool,
    request: &RunnerControlPlanePostQueueRequest,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
    mode: FixtureMode,
) -> Result<RunnerControlPlanePostQueueResult> {
    anyhow::ensure!(
        matches!(pool, DbPool::Postgres(_))
            && request.version == 1
            && request.operation == mode.post_queue_operation(),
        "fixture post-queue request"
    );
    for id in [
        &request.account_id,
        &request.application_id,
        &request.run_id,
        &request.browser_session_id,
        &request.command_id,
        &request.request_id,
        &request.workflow_id,
    ] {
        anyhow::ensure!(bounded_id(id, 1, 240), "fixture post-queue identifiers");
    }
    anyhow::ensure!(
        validate_execution_binding(&request.browser_profile_id, 160),
        "fixture browser profile"
    );
    validate_bootstrap_binding(mode, &request.account_id, bootstrap, private_key)?;
    let mut connection = pool.get_pg()?;
    let mut tx = connection
        .build_transaction()
        .isolation_level(postgres::IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()?;
    let command_row = tx.query_opt(&format!(
        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands WHERE account_id=$1 AND id=$2"),
        &[&request.account_id, &request.command_id])?.context("fixture actual command missing")?;
    let command = workflow_command_from_stored(postgres_workflow_command_row(&command_row))?;
    anyhow::ensure!(
        command.application_id == request.application_id
            && command.run_id == request.run_id
            && command.workflow_id == request.workflow_id
            && command.request_id == request.request_id
            && command.command_kind == JobsWorkflowCommandKind::Start
            && command.protocol_version == 2
            && command.state == JobsWorkflowCommandState::DeliveryUnknown
            && command.first_request_started_at_ms.is_some()
            && command.temporal_run_id.is_none()
            && command.accepted_at_ms.is_none(),
        "fixture actual request-start command binding"
    );
    let JobsWorkflowCommandPayload::Start(material) = &command.envelope.payload else {
        anyhow::bail!("fixture command is not start");
    };
    anyhow::ensure!(
        material.browser_session_id == request.browser_session_id
            && material.result_request_id == request.request_id
            && material
                .workflow_input
                .get("accountId")
                .and_then(Value::as_str)
                == Some(request.account_id.as_str())
            && material
                .workflow_input
                .get("applicationId")
                .and_then(Value::as_str)
                == Some(request.application_id.as_str())
            && material
                .workflow_input
                .get("idempotencyKey")
                .and_then(Value::as_str)
                == Some(request.run_id.as_str())
            && material
                .workflow_input
                .get("browserProfileId")
                .and_then(Value::as_str)
                == Some(request.browser_profile_id.as_str())
            && material
                .workflow_input
                .get("runner")
                .and_then(Value::as_str)
                == Some("cloud"),
        "fixture actual workflow material binding"
    );
    let request_starts: i64 = tx.query_one(
        "SELECT count(*) FROM jobs_managed_cloud_request_start_authorities a
         JOIN jobs_workflow_command_attempt_events e ON e.command_id=a.command_id AND e.attempt_id=a.attempt_id
         WHERE a.account_id=$1 AND a.command_id=$2 AND e.event_kind='request_started'",
        &[&request.account_id, &request.command_id])?.get(0);
    anyhow::ensure!(
        request_starts == 1 && command.attempt_count == 1,
        "fixture requires exactly one real dispatcher request-start"
    );
    let row = tx
        .query_opt(
            "SELECT job_id,application_json FROM jobs_applications WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.application_id],
        )?
        .context("fixture actual application missing")?;
    let job_id: String = row.get(0);
    let application = parse_application_json(
        row.get(1),
        &request.application_id,
        &job_id,
        "fixture application",
    )?;
    anyhow::ensure!(
        job_id == bootstrap.job_id
            && application.state == "queued"
            && application.submission_mode == mode.submission_mode()
            && application.run_id.as_deref() == Some(request.run_id.as_str())
            && application.submitted_at_ms.is_none(),
        "fixture ordinary queued application binding"
    );
    approved_submission_snapshot(&request.account_id, &application)?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .context("fixture approved identity missing")?;
    anyhow::ensure!(
        execution_browser_profile_id(&request.account_id, identity_id)
            == request.browser_profile_id
            && application
                .receipt
                .get("career_track_id")
                .and_then(Value::as_str)
                == Some(bootstrap.track_id.as_str()),
        "fixture approved track/profile binding"
    );
    let row = tx
        .query_opt(
            "SELECT session_json FROM jobs_browser_sessions WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.browser_session_id],
        )?
        .context("fixture ordinary session missing")?;
    let session: BrowserSession = parse_json(row.get(0), "fixture browser session")?;
    anyhow::ensure!(
        session.id == request.browser_session_id
            && session.runner == "cloud"
            && session.status == "queued"
            && session.application_id.as_deref() == Some(request.application_id.as_str())
            && session.id == format!("cloud-{}", request.application_id)
            && session.id != request.run_id,
        "fixture ordinary session identity binding"
    );
    let aliases: i64 = tx
        .query_one(
            "SELECT count(*) FROM jobs_browser_sessions WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.run_id],
        )?
        .get(0);
    let leases: i64 = tx
        .query_one(
            "SELECT count(*) FROM jobs_execution_leases WHERE account_id=$1 AND run_id=$2",
            &[&request.account_id, &request.run_id],
        )?
        .get(0);
    anyhow::ensure!(
        aliases == 0 && leases == 0,
        "fixture must not fabricate alias or preclaim"
    );
    let volume = tx.query_opt(
        "SELECT v.worker_id,v.current_epoch,v.active_instance_id,v.status,k.key_fingerprint,
         b.grant_id,b.runtime_sha256 FROM jobs_runner_volumes v
         JOIN jobs_runner_volume_keys k ON k.volume_id=v.volume_id AND k.enrollment_epoch=v.current_epoch
         JOIN jobs_runner_process_runtime_bindings b ON b.volume_id=v.volume_id
          AND b.enrollment_epoch=v.current_epoch AND b.process_instance_id=v.active_instance_id
         WHERE v.volume_id=$1", &[&bootstrap.volume.volume_id])?.context("fixture active volume missing")?;
    anyhow::ensure!(
        volume.get::<_, String>(0) == private_key.worker_id
            && volume.get::<_, i64>(1) == bootstrap.volume.enrollment_epoch
            && volume.get::<_, Option<String>>(2).as_deref()
                == Some(bootstrap.volume.process_instance_id.as_str())
            && volume.get::<_, String>(3) == "active"
            && volume.get::<_, String>(4) == bootstrap.volume.key_fingerprint
            && volume.get::<_, String>(5) == bootstrap.volume.runtime_grant_id
            && volume.get::<_, String>(6) == bootstrap.volume.runtime_sha256,
        "fixture durable volume binding"
    );
    let memo = postgres_managed_cloud_workflow_release_memo(&mut tx, &command)?
        .context("fixture actual managed memo missing")?;
    let stored_start = postgres_managed_cloud_stored_request_start_authority(&mut tx, &command.id)?
        .context("fixture actual request-start authority missing")?;
    let gateway_bytes = managed_cloud_decode_base64url(&stored_start.gateway_authority_base64url)?;
    anyhow::ensure!(
        managed_cloud_sha256(&gateway_bytes) == stored_start.gateway_authority_sha256
            && stored_start.account_id == request.account_id
            && stored_start.command_id == request.command_id,
        "fixture stored request-start authority binding"
    );
    let gateway: ManagedCloudGatewayAuthority = managed_cloud_parse_canonical(&gateway_bytes)?;
    let binding = postgres_managed_cloud_workflow_binding(&mut tx, &command.id)?
        .context("fixture actual command release binding missing")?;
    let binding = require_exact_managed_cloud_workflow_binding(
        &binding,
        &ManagedCloudWorkflowBindingInput {
            command_id: request.command_id.clone(),
            account_id: request.account_id.clone(),
            application_id: request.application_id.clone(),
            run_id: request.run_id.clone(),
            workflow_id: request.workflow_id.clone(),
            scope: fixture_scope(),
        },
    )?;
    validate_managed_cloud_gateway_authority(&gateway, &binding)?;
    anyhow::ensure!(
        gateway.execution == memo.execution
            && stored_start.binding_sha256 == memo.execution.binding_sha256,
        "fixture request-start memo equality"
    );
    let managed = bootstrap
        .runtimes
        .iter()
        .find(|entry| entry.instance.role == "managed_runner")
        .context("fixture managed runner missing")?;
    anyhow::ensure!(
        memo.version == 1
            && memo.execution.admission.scope == fixture_scope()
            && memo.execution.admission.activation_sha256 == managed.instance.activation_sha256
            && memo.execution.admission.manifest_sha256 == managed.instance.manifest_sha256,
        "fixture exact staging release memo binding"
    );
    let auto_summary = if mode == FixtureMode::AutoSubmit {
        let now = original_source_db_now_postgres(&mut tx)?;
        Some(read_auto_summary(
            &mut tx,
            &request.account_id,
            &application,
            bootstrap,
            now,
        )?)
    } else {
        None
    };
    tx.commit()?;
    Ok(RunnerControlPlanePostQueueResult {
        version: 1,
        operation: mode.post_queue_operation().into(),
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        browser_session_id: request.browser_session_id.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        command_id: request.command_id.clone(),
        request_id: request.request_id.clone(),
        workflow_id: request.workflow_id.clone(),
        submission_mode: application.submission_mode.clone(),
        managed_cloud_release: memo,
        auto_summary,
    })
}

// Inspect only frozen packet authority and its exact stored authorization row. The ordinary
// queue/claim paths own current-authority resolution; their locking resolvers cannot be called
// from this database-enforced READ ONLY handoff.
fn read_auto_summary(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    now: i64,
) -> Result<RunnerControlPlaneAutoSummary> {
    let admission = ats_frozen_application_admission(application, now)?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .context("fixture auto identity missing")?;
    let source_id = application
        .receipt
        .pointer("/career_track_policy_authority/source_resume_asset_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .context("fixture auto source missing")?;
    anyhow::ensure!(
        admission.career_track_id == bootstrap.track_id
            && admission.certification.runner_target_sha256s
                == vec![bootstrap.volume.runtime_sha256.clone()],
        "fixture auto frozen track/runtime"
    );
    let authorization_count: i64 = tx
        .query_one(
            "SELECT count(*) FROM jobs_auto_submit_authorizations
         WHERE account_id=$1 AND id=$2 AND career_track_id=$3
          AND revision_no=$4 AND authority_fingerprint=$5 AND revoked_at_ms IS NULL
          AND application_identity_id=$6 AND source_resume_asset_id=$7",
            &[
                &account_id,
                &admission.auto_authorization_id,
                &bootstrap.track_id,
                &admission.auto_authorization_revision,
                &admission.auto_authorization_fingerprint_sha256,
                &identity_id,
                &source_id,
            ],
        )?
        .get(0);
    anyhow::ensure!(
        authorization_count == 1,
        "fixture exact active auto authorization"
    );
    let attempt = tx
        .query_opt(
            "SELECT id,runner,status FROM jobs_attempt_reservations
         WHERE account_id=$1 AND application_id=$2",
            &[&account_id, &application.id],
        )?
        .context("fixture auto attempt missing")?;
    anyhow::ensure!(
        attempt.get::<_, String>(1) == "cloud"
            && matches!(attempt.get::<_, String>(2).as_str(), "reserved" | "running"),
        "fixture auto attempt binding"
    );
    Ok(RunnerControlPlaneAutoSummary {
        attempt_id: attempt.get(0),
        auto_authorization_id: admission.auto_authorization_id,
        auto_authorization_revision: admission.auto_authorization_revision,
        auto_authorization_fingerprint_sha256: admission.auto_authorization_fingerprint_sha256,
        packet_checksum_sha256: admission.packet_checksum_sha256,
        provider: admission.certification.provider,
        target_key_sha256: admission.certification.target_key_sha256,
        manifest_sha256: admission.certification.manifest_sha256,
        activation_sha256: admission.certification.activation_sha256,
        layout_set_sha256: admission.certification.layout_set_sha256,
        adapter_bundle_sha256: admission.certification.adapter_bundle_sha256,
        runner_target_sha256: bootstrap.volume.runtime_sha256.clone(),
    })
}

fn validate_inspect_request(
    request: &RunnerControlPlaneInspectAutoSubmitClaimRequest,
) -> Result<()> {
    anyhow::ensure!(
        request.version == 1
            && request.operation == "inspect_auto_submit_claim"
            && synthetic_account_id(&request.account_id)
            && [
                &request.application_id,
                &request.run_id,
                &request.browser_session_id
            ]
            .into_iter()
            .all(|id| bounded_id(id, 1, 240))
            && request.browser_session_id != request.run_id
            && validate_execution_binding(&request.browser_profile_id, 160)
            && ats_certification_hex64(&request.lease_token_sha256)
            && (1..=9_007_199_254_740_991).contains(&request.lease_fence),
        "fixture auto inspection request"
    );
    Ok(())
}

/// One post-first-claim observation. Never creates, renews, consumes, or repairs authority.
pub fn inspect_auto_submit_claim(
    pool: &DbPool,
    request: &RunnerControlPlaneInspectAutoSubmitClaimRequest,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    post_queue: &RunnerControlPlanePostQueueResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
) -> Result<RunnerControlPlaneInspectAutoSubmitClaimResult> {
    anyhow::ensure!(
        matches!(pool, DbPool::Postgres(_)),
        "fixture inspection PostgreSQL"
    );
    validate_inspect_request(request)?;
    validate_bootstrap_binding(
        FixtureMode::AutoSubmit,
        &request.account_id,
        bootstrap,
        private_key,
    )?;
    anyhow::ensure!(
        post_queue.version == 1
            && post_queue.operation == "post_queue_auto_submit"
            && post_queue.submission_mode == "auto_submit"
            && post_queue.account_id == request.account_id
            && post_queue.application_id == request.application_id
            && post_queue.run_id == request.run_id
            && post_queue.browser_session_id == request.browser_session_id
            && post_queue.browser_profile_id == request.browser_profile_id,
        "fixture inspection post-queue binding"
    );
    let expected = post_queue
        .auto_summary
        .as_ref()
        .context("fixture auto summary missing")?;
    let mut connection = pool.get_pg()?;
    let mut tx = connection
        .build_transaction()
        .isolation_level(postgres::IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()?;
    let now = original_source_db_now_postgres(&mut tx)?;
    let row = tx
        .query_opt(
            "SELECT job_id,application_json FROM jobs_applications WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.application_id],
        )?
        .context("fixture inspection application missing")?;
    let job_id: String = row.get(0);
    let application = parse_application_json(
        row.get(1),
        &request.application_id,
        &job_id,
        "fixture inspected application",
    )?;
    anyhow::ensure!(
        job_id == bootstrap.job_id
            && application.state == "queued"
            && application.submission_mode == "auto_submit"
            && application.run_id.as_deref() == Some(request.run_id.as_str())
            && application.submitted_at_ms.is_none(),
        "fixture inspected application binding"
    );
    approved_submission_snapshot(&request.account_id, &application)?;
    let observed = read_auto_summary(&mut tx, &request.account_id, &application, bootstrap, now)?;
    anyhow::ensure!(
        observed == *expected,
        "fixture inspected auto summary changed"
    );
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .context("fixture inspected identity missing")?;
    anyhow::ensure!(
        execution_browser_profile_id(&request.account_id, identity_id)
            == request.browser_profile_id,
        "fixture inspected identity/profile"
    );
    let row = tx
        .query_opt(
            "SELECT session_json FROM jobs_browser_sessions WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.browser_session_id],
        )?
        .context("fixture inspected session missing")?;
    let session: BrowserSession = parse_json(row.get(0), "fixture inspected session")?;
    anyhow::ensure!(
        session.id == request.browser_session_id
            && session.id == format!("cloud-{}", application.id)
            && session.runner == "cloud"
            && session.status == "queued"
            && session.application_id.as_deref() == Some(application.id.as_str()),
        "fixture inspected durable session"
    );
    let lease = tx.query_opt(
        "SELECT account_id,application_id,browser_profile_id,owner_id,lease_token_sha256,
          fence,phase,lease_expires_at_ms,finished_at_ms FROM jobs_execution_leases WHERE run_id=$1",
        &[&request.run_id],
    )?.context("fixture prepared lease missing")?;
    anyhow::ensure!(
        lease.get::<_, String>(0) == request.account_id
            && lease.get::<_, String>(1) == request.application_id
            && lease.get::<_, String>(2) == request.browser_profile_id
            && lease.get::<_, String>(3) == private_key.worker_id
            && lease.get::<_, String>(4) == request.lease_token_sha256
            && lease.get::<_, i64>(5) == request.lease_fence
            && lease.get::<_, String>(6) == "prepared"
            && lease.get::<_, i64>(7) > now
            && lease.get::<_, Option<i64>>(8).is_none(),
        "fixture exact prepared lease"
    );
    let counts = tx.query_one(
        "SELECT (SELECT count(*) FROM jobs_application_ats_certification_bindings WHERE account_id=$1),
          (SELECT count(*) FROM jobs_application_ats_certification_bindings WHERE application_id=$2),
          (SELECT count(*) FROM jobs_application_ats_certification_bindings WHERE run_id=$3),
          (SELECT count(*) FROM jobs_browser_sessions WHERE account_id=$1 AND id=$3)",
        &[&request.account_id, &request.application_id, &request.run_id],
    )?;
    anyhow::ensure!(
        counts.get::<_, i64>(0) == 1
            && counts.get::<_, i64>(1) == 1
            && counts.get::<_, i64>(2) == 1
            && counts.get::<_, i64>(3) == 0,
        "fixture exact Phase A cardinality/no alias"
    );
    let row = tx.query_one(
        "SELECT * FROM jobs_application_ats_certification_bindings
         WHERE account_id=$1 AND application_id=$2 AND run_id=$3",
        &[
            &request.account_id,
            &request.application_id,
            &request.run_id,
        ],
    )?;
    let binding_sha256: String = row.get("binding_sha256");
    let authority = decode_ats_application_binding_authority(
        &binding_sha256,
        row.get::<_, String>("frozen_certification_base64url")
            .as_str(),
    )?;
    let phase: String = row.get("phase");
    let binding_fence: i64 = row.get("fence");
    for field in [
        "layout_observation_sha256",
        "observed_surface_sha256",
        "phase_b_request_id",
        "phase_b_request_sha256",
        "canary_reservation_sha256",
        "metering_reservation_sha256",
        "invalidation_kind",
    ] {
        anyhow::ensure!(
            row.get::<_, Option<String>>(field).is_none(),
            "fixture Phase B/invalidation absent"
        );
    }
    anyhow::ensure!(
        phase == "preflight"
            && binding_fence == 0
            && row.get::<_, Option<i64>>("consumed_at_ms").is_none()
            && row.get::<_, Option<i64>>("invalidated_at_ms").is_none()
            && row.get::<_, i64>("expires_at_ms") == authority.expires_at_ms
            && row.get::<_, i64>("created_at_ms") == authority.created_at_ms
            && authority.expires_at_ms > now
            && authority.requested_expires_at_ms >= authority.expires_at_ms,
        "fixture unconsumed future Phase A"
    );
    let trusted_row = tx.query_opt(&format!(
        "SELECT {TRUSTED_PROCESS_RUNTIME_COLUMNS} FROM jobs_execution_lease_process_runtime_bindings e
         JOIN jobs_runner_process_runtime_bindings b ON b.grant_id=e.runtime_grant_id
          AND b.worker_id=e.worker_id AND b.volume_id=e.volume_id AND b.enrollment_epoch=e.enrollment_epoch
          AND b.process_instance_id=e.process_instance_id AND b.runtime_sha256=e.runtime_sha256
         JOIN jobs_runner_process_runtime_grants g ON g.grant_id=b.grant_id
         WHERE e.run_id=$1 AND e.fence=$2"), &[&request.run_id, &request.lease_fence])?
        .context("fixture execution runtime binding missing")?;
    let trusted = trusted_process_runtime_from_pg_row(trusted_row);
    validate_trusted_process_runtime(&trusted)?;
    anyhow::ensure!(
        exact_trusted_process_binding(
            &trusted,
            &bootstrap.volume.runtime_grant_id,
            &private_key.worker_id,
            &bootstrap.volume.volume_id,
            bootstrap.volume.enrollment_epoch,
            &bootstrap.volume.process_instance_id,
            &bootstrap.volume.runtime_sha256
        ),
        "fixture exact execution runtime"
    );
    let runtime = runtime_target(&trusted.runtime)?;
    let posting_row = tx.query_one(
        "SELECT posting_json FROM jobs_postings WHERE account_id=$1 AND id=$2",
        &[&request.account_id, &job_id],
    )?;
    let posting: JobPosting = parse_json(posting_row.get(0), "fixture Phase A posting")?;
    let target = ats_certification_fresh_target_evidence_from_posting(&posting, now)?;
    let admission = ats_frozen_application_admission(&application, now)?;
    let context = AtsCertificationPhaseAContextRequest {
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        browser_session_id: request.browser_session_id.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        runtime_attestation: cloud_ats_runtime_attestation(&trusted),
        nonce_sha256: request.lease_token_sha256.clone(),
    };
    anyhow::ensure!(
        authority.schema_version == 1
            && authority.binding_id == row.get::<_, String>("binding_id")
            && authority.binding_id == ats_phase_a_binding_id(&context, &expected.attempt_id)
            && authority.account_id == request.account_id
            && authority.application_id == request.application_id
            && authority.run_id == request.run_id
            && authority.application_attempt_id == expected.attempt_id
            && authority.browser_session_id == request.browser_session_id
            && authority.browser_profile_id == request.browser_profile_id
            && authority.packet_checksum_sha256 == expected.packet_checksum_sha256
            && authority.auto_authorization_id == expected.auto_authorization_id
            && authority.auto_authorization_revision == expected.auto_authorization_revision
            && authority.auto_authorization_fingerprint_sha256
                == expected.auto_authorization_fingerprint_sha256
            && authority.nonce_sha256 == request.lease_token_sha256
            && authority.target_evidence == target
            && authority.created_at_ms <= now
            && authority.certification.channel == "general"
            && authority.certification.selected_runtime.as_ref() == Some(&runtime)
            && ats_frozen_certification_admission_projection(&authority.certification)?
                == admission.certification,
        "fixture exact frozen Phase A authority"
    );
    for (field, value) in [
        ("account_id", request.account_id.as_str()),
        ("application_id", request.application_id.as_str()),
        ("run_id", request.run_id.as_str()),
        ("attempt_id", expected.attempt_id.as_str()),
        ("browser_session_id", request.browser_session_id.as_str()),
        ("browser_profile_id", request.browser_profile_id.as_str()),
        (
            "packet_checksum_sha256",
            expected.packet_checksum_sha256.as_str(),
        ),
        (
            "auto_authorization_id",
            expected.auto_authorization_id.as_str(),
        ),
        (
            "auto_authorization_fingerprint_sha256",
            expected.auto_authorization_fingerprint_sha256.as_str(),
        ),
        ("provider", expected.provider.as_str()),
        ("target_key", authority.certification.target_key.as_str()),
        ("manifest_sha256", expected.manifest_sha256.as_str()),
        ("activation_sha256", expected.activation_sha256.as_str()),
        ("layout_set_sha256", expected.layout_set_sha256.as_str()),
        (
            "adapter_bundle_sha256",
            expected.adapter_bundle_sha256.as_str(),
        ),
        (
            "runner_target_sha256",
            expected.runner_target_sha256.as_str(),
        ),
        ("browser_runtime_sha256", runtime.runtime_sha256.as_str()),
        ("platform", runtime.platform.as_str()),
        ("architecture", runtime.architecture.as_str()),
        (
            "automation_bundle_sha256",
            runtime.automation_bundle_sha256.as_str(),
        ),
        (
            "chromium_executable_sha256",
            runtime.chromium_executable_sha256.as_str(),
        ),
        ("nonce_sha256", request.lease_token_sha256.as_str()),
    ] {
        anyhow::ensure!(
            row.get::<_, String>(field) == value,
            "fixture Phase A stored projection"
        );
    }
    anyhow::ensure!(
        row.get::<_, i64>("auto_authorization_revision") == expected.auto_authorization_revision
            && row.get::<_, Option<String>>("runner_build_id") == runtime.runner_build_id
            && row.get::<_, Option<String>>("runner_image_sha256") == runtime.runner_image_sha256
            && row
                .get::<_, Option<String>>("browser_release_manifest_sha256")
                .is_none()
            && row
                .get::<_, Option<String>>("browser_artifact_sha256")
                .is_none()
            && row
                .get::<_, Option<String>>("browser_build_descriptor_sha256")
                .is_none(),
        "fixture Phase A stored cloud runtime"
    );
    tx.commit()?;
    Ok(RunnerControlPlaneInspectAutoSubmitClaimResult {
        version: 1,
        operation: "inspect_auto_submit_claim",
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        browser_session_id: request.browser_session_id.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        lease_token_sha256: request.lease_token_sha256.clone(),
        lease_fence: request.lease_fence,
        ats_account_count: 1,
        ats_application_count: 1,
        ats_run_count: 1,
        binding_sha256,
        nonce_sha256: authority.nonce_sha256,
        phase,
        binding_fence,
        expires_future: true,
        exact_frozen_authority: true,
        exact_prepared_lease: true,
        no_phase_b_or_invalidation: true,
    })
}

fn validate_checkpoint_inspection_request(
    request: &RunnerControlPlaneInspectCheckpointRecoveryRequest,
) -> Result<()> {
    anyhow::ensure!(
        request.version == 1
            && request.operation == "inspect_checkpoint_recovery"
            && synthetic_account_id(&request.account_id)
            && [
                &request.application_id,
                &request.run_id,
                &request.browser_session_id
            ]
            .into_iter()
            .all(|id| bounded_id(id, 1, 240))
            && request.browser_session_id != request.run_id
            && validate_execution_binding(&request.browser_profile_id, 160),
        "fixture checkpoint inspection request"
    );
    Ok(())
}

fn canonical_fixture_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonical_fixture_value).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), canonical_fixture_value(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        other => other.clone(),
    }
}

fn canonical_fixture_sha256(value: &Value) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(
        &canonical_fixture_value(value),
    )?)))
}

fn stable_application_sha256(application: &JobApplication) -> Result<String> {
    let mut value = serde_json::to_value(application)?;
    let object = value
        .as_object_mut()
        .context("fixture checkpoint application encoding")?;
    anyhow::ensure!(
        object.remove("state").is_some() && object.remove("updated_at_ms").is_some(),
        "fixture checkpoint application mutable fields"
    );
    object
        .get_mut("receipt")
        .and_then(Value::as_object_mut)
        .context("fixture checkpoint application receipt")?
        .remove("cloud_recovery");
    canonical_fixture_sha256(&value)
}

fn stable_session_sha256(session: &BrowserSession) -> Result<String> {
    let mut value = serde_json::to_value(session)?;
    let object = value
        .as_object_mut()
        .context("fixture checkpoint session encoding")?;
    for field in ["status", "current_step", "updated_at_ms", "takeover_url"] {
        anyhow::ensure!(
            object.remove(field).is_some(),
            "fixture checkpoint session mutable fields"
        );
    }
    canonical_fixture_sha256(&value)
}

fn checkpoint_cloud_recovery(
    stage: RunnerControlPlaneCheckpointStage,
    application: &JobApplication,
) -> Result<(
    Option<RunnerControlPlaneCloudRecoverySummary>,
    Option<String>,
)> {
    let Some(value) = application.receipt.get("cloud_recovery") else {
        anyhow::ensure!(stage.is_before(), "fixture checkpoint recovery missing");
        return Ok((None, None));
    };
    anyhow::ensure!(!stage.is_before(), "fixture checkpoint recovery premature");
    let summary: RunnerControlPlaneCloudRecoverySummary = serde_json::from_value(value.clone())?;
    anyhow::ensure!(
        summary.schema_version == 1
            && summary.status == "released"
            && summary.checkpoint_version == 2
            && summary.checkpoint_phase == "prepared"
            && summary.lease_phase == "prepared"
            && (1..=9_007_199_254_740_991).contains(&summary.recorded_at_ms)
            && summary.run_id == application.run_id.as_deref().unwrap_or_default(),
        "fixture checkpoint recovery summary"
    );
    let sha256 = canonical_fixture_sha256(&serde_json::to_value(&summary)?)?;
    Ok((Some(summary), Some(sha256)))
}

/// Read-only proof that checkpoint recovery changed only the permitted application/session
/// lifecycle fields. Existing Node snapshots remain authoritative for all other table families.
pub fn inspect_checkpoint_recovery(
    pool: &DbPool,
    request: &RunnerControlPlaneInspectCheckpointRecoveryRequest,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    post_queue: &RunnerControlPlanePostQueueResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
) -> Result<RunnerControlPlaneInspectCheckpointRecoveryResult> {
    anyhow::ensure!(
        matches!(pool, DbPool::Postgres(_)),
        "fixture checkpoint inspection PostgreSQL"
    );
    validate_checkpoint_inspection_request(request)?;
    let mut connection = pool.get_pg()?;
    let mut tx = connection
        .build_transaction()
        .isolation_level(postgres::IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()?;
    let row = tx
        .query_opt(
            "SELECT job_id,application_json FROM jobs_applications
             WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.application_id],
        )?
        .context("fixture checkpoint application missing")?;
    let job_id: String = row.get(0);
    let application = parse_application_json(
        row.get(1),
        &request.application_id,
        &job_id,
        "fixture checkpoint application",
    )?;
    let mode = FixtureMode::from_submission_mode(&application.submission_mode)?;
    validate_bootstrap_binding(mode, &request.account_id, bootstrap, private_key)?;
    anyhow::ensure!(
        bootstrap.job_id == job_id
            && post_queue.version == 1
            && post_queue.operation == mode.post_queue_operation()
            && post_queue.account_id == request.account_id
            && post_queue.application_id == request.application_id
            && post_queue.run_id == request.run_id
            && post_queue.browser_session_id == request.browser_session_id
            && post_queue.browser_profile_id == request.browser_profile_id
            && post_queue.submission_mode == mode.submission_mode()
            && post_queue.auto_summary.is_some() == (mode == FixtureMode::AutoSubmit)
            && application.id == request.application_id
            && application.run_id.as_deref() == Some(request.run_id.as_str())
            && application.submitted_at_ms.is_none(),
        "fixture checkpoint file/application binding"
    );
    approved_submission_snapshot(&request.account_id, &application)?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .context("fixture checkpoint identity missing")?;
    anyhow::ensure!(
        execution_browser_profile_id(&request.account_id, identity_id)
            == request.browser_profile_id,
        "fixture checkpoint identity/profile"
    );

    let row = tx
        .query_opt(
            "SELECT session_json FROM jobs_browser_sessions WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.browser_session_id],
        )?
        .context("fixture checkpoint session missing")?;
    let session: BrowserSession = parse_json(row.get(0), "fixture checkpoint session")?;
    anyhow::ensure!(
        session.id == request.browser_session_id
            && session.id == format!("cloud-{}", application.id)
            && session.id != request.run_id
            && session.runner == "cloud"
            && session.application_id.as_deref() == Some(request.application_id.as_str())
            && session.takeover_url.is_none(),
        "fixture checkpoint session binding"
    );

    let lease = tx
        .query_opt(
            "SELECT account_id,application_id,browser_profile_id,owner_id,phase
             FROM jobs_execution_leases WHERE run_id=$1",
            &[&request.run_id],
        )?
        .context("fixture checkpoint lease missing")?;
    let lease_phase: String = lease.get(4);
    anyhow::ensure!(
        lease.get::<_, String>(0) == request.account_id
            && lease.get::<_, String>(1) == request.application_id
            && lease.get::<_, String>(2) == request.browser_profile_id
            && lease.get::<_, String>(3) == private_key.worker_id,
        "fixture checkpoint lease binding"
    );
    let attempt = tx
        .query_opt(
            "SELECT runner,status FROM jobs_attempt_reservations
             WHERE account_id=$1 AND application_id=$2",
            &[&request.account_id, &request.application_id],
        )?
        .context("fixture checkpoint attempt missing")?;
    let attempt_status: String = attempt.get(1);
    anyhow::ensure!(
        attempt.get::<_, String>(0) == "cloud",
        "fixture checkpoint attempt binding"
    );

    if request.stage.is_before() {
        anyhow::ensure!(
            matches!(application.state.as_str(), "queued" | "running")
                && matches!(session.status.as_str(), "queued" | "running")
                && lease_phase == "prepared"
                && matches!(attempt_status.as_str(), "reserved" | "running"),
            "fixture checkpoint before state"
        );
    } else {
        anyhow::ensure!(
            application.state == "failed"
                && session.status == "failed"
                && session.current_step == "Browser run stopped before submission"
                && lease_phase == "released"
                && attempt_status == "released",
            "fixture checkpoint recovered state"
        );
    }
    let (cloud_recovery, cloud_recovery_sha256) =
        checkpoint_cloud_recovery(request.stage, &application)?;
    let application_stable_sha256 = stable_application_sha256(&application)?;
    let session_stable_sha256 = stable_session_sha256(&session)?;
    tx.commit()?;
    Ok(RunnerControlPlaneInspectCheckpointRecoveryResult {
        version: 1,
        operation: "inspect_checkpoint_recovery",
        stage: request.stage,
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        browser_session_id: request.browser_session_id.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        submission_mode: application.submission_mode,
        application_stable_sha256,
        session_stable_sha256,
        application_state: application.state,
        session_status: session.status,
        session_current_step: session.current_step,
        submitted_absent: true,
        takeover_absent: true,
        lease_phase,
        attempt_status,
        cloud_recovery,
        cloud_recovery_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // All of this factory's records/signatures are UNIT METADATA ONLY. They do
    // not represent a VM, real CI, native storage, a runner startup or fleet proof.
    fn unit_startup_request_json() -> Value {
        let envelope =
            json!({"canonicalBase64url":"unit-only", "signatureSetBase64url":"unit-only"});
        json!({"version":1,"operation":"issue_measured_runner_startup",
            "worker_id":"phase641-unit-worker-00000001","issuance_ref":"phase641-unit-issuance",
            "provider_resource_id":"phase641-owned-unit-volume","resource_fingerprint":"a".repeat(64),
            "authorization_ref":"phase641-unit-only-authority","ttl_ms":60_000,
            "process_runtime":runtime_model("phase641-unit-only"),"startup_evidence_base64url":"unit-only",
            "release_test_evidence_base64url":"unit-only","trust_policy":envelope,
            "release":{"envelope":envelope,"verificationEvidenceBase64url":"unit-only","inventoryAttachments":{
                "jobs-api":"unit-only","jobs-portal":"unit-only","jobs-runner":"unit-only","jobs-workflows":"unit-only"}},
            "cohort":{"envelope":envelope,"accountIds":[]},"activation":{"envelope":envelope,"evidence":{
                "canaryBase64url":"unit-only","cleanupBase64url":"unit-only","failureConverterBase64url":"unit-only",
                "portalReadbackBase64url":"unit-only","runnerFleetBase64url":"unit-only","storageBase64url":"unit-only",
                "taskQueueBase64url":"unit-only","temporalNamespaceBase64url":"unit-only"}}})
    }

    fn unit_startup_request() -> MeasuredRunnerStartupRequest {
        serde_json::from_value(unit_startup_request_json()).unwrap()
    }

    fn unit_d1_expected() -> MeasuredRunnerD1Expected {
        // Planned C admission pins, not actual D1 execution evidence. The inspection
        // aggregate is a unit-only value because these fixtures contain no tool bytes.
        serde_json::from_value(json!({
            "version":1,
            "source_commit":"14d56e770670291ab12798888c45c851d34efe42",
            "source_branch":"feat/phase-646-jobs-current-source-runner-qualification",
            "source_file_count":846,
            "source_manifest_sha256":"a93f795803c6b224879f5f92ee385000db16449c8afd8c7d66147e3a6b305ec1",
            "source_archive_sha256":"5366d56177945607b2754915bb1000b9d10f27df1ddb2715d2cd74da09385a60",
            "controller_source_sha256":"9472b523a6888410bf958cf90a38ebca7113d522e8b245a5dd54aab8084fa221",
            "harness_sha256":"5f3d8831212afa9e72ac7d5d574d58ab465431a53a7636b92e888e626f73a846",
            "vm_test_sha256":"3331113d5a99f8278cd2ccccfe0e5a4afa136af634547ae89084184f9b95c82b",
            "artifact_helper_sha256":"f76951933fc950d788fc624f80a8ee48290907f3fac9657e7fd5c5e2ee0d238c",
            "inspector_sha256":"27532f5b47305fd363c8168bb4078bffd165365d243aa26a498177e0f5ea87b6",
            "image_tool_sources_sha256":"17ce99c0238a28f8e8ec19ea08c3ad05121a68b25c1a068155da20f9aff8b985",
            "inspection_tool_sources_sha256":"5".repeat(64)
        })).unwrap()
    }

    fn unit_d1_metadata() -> MeasuredRunnerD1Readback {
        unit_d1_metadata_for(&unit_d1_expected())
    }

    fn unit_d1_metadata_for(admission: &MeasuredRunnerD1Expected) -> MeasuredRunnerD1Readback {
        let commit = &admission.source_commit;
        let measurement = json!({"version":1,"audience":MANAGED_CLOUD_RUNTIME_MEASUREMENT_AUDIENCE,
            "buildId":format!("nonprod-p639-{}",&commit[..12]),"componentId":"jobs-runner",
            "configSchemaSha256":"c".repeat(64),"migrationSetSha256":"e".repeat(64),"protocolSetSha256":"f".repeat(64),
            "roles":["managed_runner"],"sourceCommit":commit,"measuredFiles":[
                {"path":"app/automation/dist/index.js","sha256":"a".repeat(64)},
                {"path":"app/automation/package.json","sha256":"b".repeat(64)},
                {"path":"ms-playwright/chromium_headless_shell-1228/linux64/chrome-headless-shell","sha256":"d".repeat(64)}]});
        let measurement_bytes = managed_cloud_canonical_json(&measurement).unwrap();
        let measurement_sha = managed_cloud_sha256(&measurement_bytes);
        let bundle = managed_cloud_digest(
            &json!({"audience":"bluey-jobs-managed-runner-automation-bundle-v1",
            "version":1,"entries":measurement["measuredFiles"].as_array().unwrap()[..2].to_vec()}),
        )
        .unwrap();
        let inventory =
            managed_cloud_canonical_json(&json!({"unitOnly":"not-a-release-inventory"})).unwrap();
        let inspection = json!({"version":1,"expected":{"version":1,"sourceCommit":commit,
            "buildId":measurement["buildId"],"configSchemaSha256":measurement["configSchemaSha256"],
            "migrationSetSha256":measurement["migrationSetSha256"],"protocolSetSha256":measurement["protocolSetSha256"]},
            "oci":{"manifestSha256":"1".repeat(64),"imageConfigSha256":"2".repeat(64),"inventorySha256":managed_cloud_sha256(&inventory)},
            "runtime":{"runtimeMeasurementSha256":measurement_sha,"automationBundleSha256":bundle,
                "runtimeIdentitySha256":managed_cloud_runtime_identity_sha256(&measurement_sha,"jobs-runner","managed_runner").unwrap()},
            "native":{"writeReadback":true,"inventory":true,"removed":true},"browser":{"playwrightVersion":"1.61.1",
                "chromiumVersion":"130.0.0","chromiumRevision":"1228",
                "executablePath":"/ms-playwright/chromium_headless_shell-1228/linux64/chrome-headless-shell",
                "executableSha256":"d".repeat(64),"titleReadback":true,"requestsAborted":0,
                "launchProfile":"production_default_options_synthetic_only"},"inspectorSha256":admission.inspector_sha256,
            "claims":{"actualImageInspection":true,"nativeAddon":true,"browserOnly":true,
                "normalRunnerStartup":false,"managedAuthority":false,"submission":false,"production":false}});
        let inspection_bytes = managed_cloud_canonical_json(&inspection).unwrap();
        let source = json!({"sourceCommit":commit,"sourceArchiveSha256":admission.source_archive_sha256,
            "toolSourcesSha256":admission.inspection_tool_sources_sha256,
            "harnessSha256":admission.harness_sha256,"imageToolSourcesSha256":admission.image_tool_sources_sha256});
        let binding = json!({"parentIdentity":{"dev":"1","ino":"1"},"childIdentity":{"dev":"1","ino":"2"},
            "childName":"phase639-image-artifacts-000000000000000000000001","hostRuntime":{"unitOnly":true}});
        let names = [
            "runner.oci.tar",
            "inventory.json",
            "measurement.json",
            "report.json",
        ];
        let hashes = [
            managed_cloud_sha256(b"unit archive"),
            managed_cloud_sha256(&inventory),
            measurement_sha,
            managed_cloud_sha256(&inspection_bytes),
        ];
        let sizes = [
            12,
            inventory.len(),
            measurement_bytes.len(),
            inspection_bytes.len(),
        ];
        let files: Vec<_> = names.iter().enumerate().map(|(index,name)| json!({"name":name,"size":sizes[index],
            "sha256":hashes[index],"identity":{"dev":"1","ino":(index+1).to_string(),"uid":501,"mode":384,
                "nlink":1,"size":sizes[index],"mtimeNs":"1","ctimeNs":"1"}})).collect();
        let retained_files: Vec<_> = files.iter().map(|file|json!({"name":file["name"],"size":file["size"],
            "sha256":file["sha256"],"identity":{"dev":file["identity"]["dev"],"ino":file["identity"]["ino"]}})).collect();
        let manifest = json!({"version":1,"state":"attachments_complete","qualified":false,"d2Authority":false,"releaseAuthority":false,
            "binding":binding,"source":source,"reader":{"runtime":{"unitOnly":true},"readerSourceSha256":admission.artifact_helper_sha256,
                "continuity":{"unitOnly":true}},"files":files,"inspection":inspection});
        let started = json!({"version":1,"state":"incomplete","qualified":false,"d2Authority":false,"releaseAuthority":false,
            "binding":binding,"source":source,"artifacts":names});
        let record = |value: &Value| {
            let mut bytes = managed_cloud_canonical_json(value).unwrap();
            let _ = bytes.pop();
            bytes
        };
        let manifest_bytes = record(&manifest);
        let cleanup = json!({"privateInventoriesEmpty":true,"ownedVmPidsAbsent":true,"socketAbsent":true,"stateAbsent":true,"usernetSocketsAbsent":true});
        let compatibility = json!({"baseCommit":commit,"cleanup":cleanup,
            "harnessSha256":admission.harness_sha256,"testSha256":admission.vm_test_sha256});
        let image = json!({"sourceCommit":commit,"sourceArchiveSha256":source["sourceArchiveSha256"],
            "toolSourcesSha256":source["toolSourcesSha256"],"inspection":inspection,"cleanup":{"builderRemoved":true,"containerRemoved":true}});
        let claims = json!({"normalRunnerStartup":false,"managedAuthority":false,"submission":false,"production":false});
        let raw_report = format!("{{\"event\":\"managed_runner_vm_image_smoke_finished\",\"version\":1,\"qualification\":\"isolated_measured_runner_image_only\",\"compatibility\":{compatibility},\"image\":{image},\"imageToolSourcesSha256\":{},\"claims\":{claims}}}\n",source["imageToolSourcesSha256"]);
        let qualification = json!({"version":1,"state":"d1_qualified","qualification":"isolated_measured_runner_image_only",
            "d2Authority":false,"releaseAuthority":false,"externalTerminalZeroRequired":true,
            "exportManifestSha256":managed_cloud_sha256(&manifest_bytes),"proof":{"sourceCommit":commit,
                "imageReportSha256":managed_cloud_sha256(raw_report.as_bytes()),"harnessSha256":source["harnessSha256"],
                "toolSourcesSha256":source["imageToolSourcesSha256"],"postListenerChecked":true,"sourceUnchanged":true,"cleanup":cleanup}});
        let qualification_bytes = record(&qualification);
        let snapshot = json!({"head":commit,"branch":admission.source_branch,
            "files":admission.source_file_count,"manifest":admission.source_manifest_sha256,
            "rawStatus":"8".repeat(64),"selfSha256":admission.controller_source_sha256});
        let state = json!({"unitOnly":true,"absent":true});
        let mut records = Vec::new();
        for event in ["source_before", "dispatch", "owned_vm_state_observed"] {
            let mut value = if event == "source_before" {
                snapshot.clone()
            } else {
                json!({})
            };
            value["event"] = json!(event);
            records.push(format!("{value}\n"));
        }
        for phase in [
            "vm_starting",
            "vm_isolation_checked",
            "inputs_prepared",
            "builder_started",
            "image_build_started",
            "image_build_finished",
            "image_inspection_started",
            "image_inspection_finished",
        ] {
            records.push(format!(
                "{}\n",
                json!({"event":"image_progress","phase":phase})
            ));
        }
        records.push(format!("{}\n",json!({"event":"command_terminal","code":0,"signal":null,"overflow":false,"observerFailure":null})));
        records.push(raw_report.replacen(
            "\"version\":1",
            "\"time\":\"unit-only\",\"version\":1",
            1,
        ));
        records.push(format!("{}\n",json!({"event":"residue","states":[state],"parentEmpty":true,"parentAbsent":true,"preexistingStateCount":0})));
        let mut after = snapshot.clone();
        after["event"] = json!("source_after");
        records.push(format!("{after}\n"));
        records.push(format!("{}\n",json!({"event":"retained_artifact_proof","manifestSha256":managed_cloud_sha256(&manifest_bytes),
            "qualificationSha256":managed_cloud_sha256(&qualification_bytes),"sourceCommit":commit,"d2Authority":false,"releaseAuthority":false,"files":retained_files})));
        after["event"] = json!("source_after_artifacts");
        records.push(format!("{after}\n"));
        records.push(format!(
            "{}\n",
            json!({"event":"controller_finished","time":"2026-11-02T00:00:00.000Z",
                "success":true,"d1Qualified":true,"qualified":false,"d2Authority":false,
                "releaseAuthority":false,"actualTerminal":{"code":0,"signal":null},
                "failure":null,"originalFailure":null,"failedArchiveRetained":false,"elapsedMs":1})
        ));
        let journal = records.concat().into_bytes();
        let stdout = format!("{}\n",json!({"event":"phase639_image_controller_finished","success":true,"journal":"unit-only.jsonl",
            "journalSha256":managed_cloud_sha256(&journal),"terminal":{"code":0,"signal":null},"states":[state],"parentAbsent":true})).into_bytes();
        let external = format!("{}\n",json!({"version":1,"controllerSourceSha256":admission.controller_source_sha256,
            "controllerFinalStdoutSha256":managed_cloud_sha256(&stdout),"controllerJournalSha256":managed_cloud_sha256(&journal),"code":0,"signal":null})).into_bytes();
        MeasuredRunnerD1Readback {
            export_started: record(&started),
            export_manifest: manifest_bytes,
            qualification: qualification_bytes,
            inventory,
            measurement: measurement_bytes,
            inspection: inspection_bytes,
            controller_journal: journal,
            controller_final_stdout: stdout,
            external_terminal: external,
            archive_sha256: hashes[0].clone(),
            archive_size: 12,
        }
    }

    #[test]
    fn measured_startup_unit_metadata_readback_binds_all_files_and_external_terminal() {
        let metadata = unit_d1_metadata();
        let binding =
            validate_measured_runner_d1(&metadata, &unit_d1_expected(), "runner-641.0").unwrap();
        assert_eq!(binding.source_commit, unit_d1_expected().source_commit);
        assert_eq!(binding.process_runtime.runner_build_id, "runner-641.0");
        assert_ne!(
            binding.process_runtime.runner_build_id,
            binding.measurement.build_id
        );
        assert_eq!(
            binding.process_runtime.automation_bundle_sha256.as_str(),
            d1_json(&metadata.inspection, 16_384, true).unwrap()["runtime"]
                ["automationBundleSha256"]
                .as_str()
                .unwrap()
        );
        for index in 0..9 {
            let mut changed = unit_d1_metadata();
            let bytes = match index {
                0 => &mut changed.export_started,
                1 => &mut changed.export_manifest,
                2 => &mut changed.qualification,
                3 => &mut changed.inventory,
                4 => &mut changed.measurement,
                5 => &mut changed.inspection,
                6 => &mut changed.controller_journal,
                7 => &mut changed.controller_final_stdout,
                _ => &mut changed.external_terminal,
            };
            bytes[0] ^= 1;
            assert!(
                validate_measured_runner_d1(&changed, &unit_d1_expected(), "runner-641.0").is_err()
            );
        }
        let mut changed = unit_d1_metadata();
        changed.archive_sha256 = "0".repeat(64);
        assert!(
            validate_measured_runner_d1(&changed, &unit_d1_expected(), "runner-641.0").is_err()
        );
        let mut changed = unit_d1_metadata();
        changed.archive_size += 1;
        assert!(
            validate_measured_runner_d1(&changed, &unit_d1_expected(), "runner-641.0").is_err()
        );
        assert!(validate_measured_runner_d1(
            &unit_d1_metadata(),
            &unit_d1_expected(),
            "nonprod-p639-not-a-deployment-token"
        )
        .is_err());
    }

    #[test]
    fn measured_startup_expected_d1_schema_and_canonical_bounds_are_closed() {
        let valid = serde_json::to_value(unit_d1_expected()).unwrap();
        let parse = |value: Value| -> Result<()> {
            serde_json::from_value::<MeasuredRunnerD1Expected>(value)?.validate()
        };
        parse(valid.clone()).unwrap();
        for key in valid.as_object().unwrap().keys() {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(parse(missing).is_err(), "missing {key}");
            let mut null = valid.clone();
            null[key] = Value::Null;
            assert!(parse(null).is_err(), "null {key}");
        }
        let mut extra = valid.clone();
        extra["qualified"] = json!(true);
        assert!(parse(extra).is_err());
        for (key, value) in [
            ("version", json!(0)),
            ("source_file_count", json!(0)),
            ("source_file_count", json!(100_001)),
            ("source_file_count", json!(846.5)),
            ("source_commit", json!("0".repeat(40))),
            ("source_commit", json!("A".repeat(40))),
            ("source_commit", json!("a".repeat(39))),
            ("source_branch", json!("")),
            ("source_branch", json!("a".repeat(256))),
            ("source_branch", json!("feat//branch")),
            ("source_branch", json!("feat/../branch")),
            ("source_branch", json!("feat/branch.lock")),
            ("source_branch", json!("feat/branch\n")),
        ] {
            let mut changed = valid.clone();
            changed[key] = value;
            assert!(parse(changed).is_err(), "malformed {key}");
        }
        for key in valid
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| key.ends_with("_sha256"))
        {
            for value in ["0".repeat(64), "A".repeat(64), "a".repeat(63)] {
                let mut changed = valid.clone();
                changed[key] = json!(value);
                assert!(parse(changed).is_err(), "malformed {key}");
            }
        }
    }

    #[test]
    fn measured_startup_expected_d1_rejects_each_mismatch_and_coherent_self_rewrite() {
        let expected = unit_d1_expected();
        let original = serde_json::to_value(&expected).unwrap();
        for key in original
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| key.as_str() != "version")
        {
            let mut rewritten = original.clone();
            rewritten[key] = match key.as_str() {
                "source_commit" => json!("2".repeat(40)),
                "source_branch" => json!("feat/different-reviewed-closure"),
                "source_file_count" => json!(expected.source_file_count + 1),
                _ => json!("2".repeat(64)),
            };
            let changed: MeasuredRunnerD1Expected = serde_json::from_value(rewritten).unwrap();
            changed.validate().unwrap();
            assert!(
                validate_measured_runner_d1(&unit_d1_metadata(), &changed, "runner-641.0").is_err(),
                "expected mismatch {key}"
            );
            // Every dependent hash and journal/report field is regenerated coherently.
            // Self-consistency still cannot replace the independent expected tuple.
            let coherent = unit_d1_metadata_for(&changed);
            validate_measured_runner_d1(&coherent, &changed, "runner-641.0").unwrap();
            assert!(
                validate_measured_runner_d1(&coherent, &expected, "runner-641.0").is_err(),
                "coherent rewrite {key}"
            );
        }
    }

    #[test]
    fn measured_startup_expected_d1_synthetic_old_shaped_hybrid_cannot_cross_replay() {
        let current = unit_d1_expected();
        // Intentionally synthetic hybrid: only these old-shaped source/controller
        // fields differ. The remaining closure is current unit metadata, not a
        // representation or validation of any actual historical D1 execution.
        let mut old = current.clone();
        old.source_commit = "32ca96dbfbe151b1ca0294ad945ce58a4276087f".into();
        old.source_branch = "feat/phase-639-jobs-managed-runtime-startup".into();
        old.source_file_count = 786;
        old.source_manifest_sha256 =
            "1421c5adf6f6135818380d2aa48697a01cf008dd23ea53fdf4d060c929562e36".into();
        old.controller_source_sha256 =
            "0ff5260db4e56f63dbf9a456b1cc1b708919dc1f795d90216c1242155b905b4c".into();
        let current_metadata = unit_d1_metadata_for(&current);
        let old_metadata = unit_d1_metadata_for(&old);
        validate_measured_runner_d1(&current_metadata, &current, "runner-641.0").unwrap();
        validate_measured_runner_d1(&old_metadata, &old, "runner-641.0").unwrap();
        assert!(validate_measured_runner_d1(&current_metadata, &old, "runner-641.0").is_err());
        assert!(validate_measured_runner_d1(&old_metadata, &current, "runner-641.0").is_err());
    }

    fn unit_rebind_controller_journal(metadata: &mut MeasuredRunnerD1Readback, records: &[Value]) {
        metadata.controller_journal = records
            .iter()
            .map(|row| format!("{row}\n"))
            .collect::<String>()
            .into_bytes();
        let mut stdout: Value = serde_json::from_slice(&metadata.controller_final_stdout).unwrap();
        stdout["journalSha256"] = json!(managed_cloud_sha256(&metadata.controller_journal));
        metadata.controller_final_stdout = format!("{stdout}\n").into_bytes();
        let mut terminal: Value = serde_json::from_slice(&metadata.external_terminal).unwrap();
        terminal["controllerJournalSha256"] = stdout["journalSha256"].clone();
        terminal["controllerFinalStdoutSha256"] =
            json!(managed_cloud_sha256(&metadata.controller_final_stdout));
        metadata.external_terminal = format!("{terminal}\n").into_bytes();
    }

    #[test]
    fn measured_startup_expected_d1_journal_self_and_finished_claims_are_exact() {
        let expected = unit_d1_expected();
        for case in 0..24 {
            let mut metadata = unit_d1_metadata();
            let manifest = d1_json(&metadata.export_manifest, 65_536, false).unwrap();
            let qualification = d1_json(&metadata.qualification, 65_536, false).unwrap();
            let mut records: Vec<Value> = std::str::from_utf8(&metadata.controller_journal)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            let event = if matches!(case, 0 | 18) {
                "source_before"
            } else {
                "controller_finished"
            };
            let row = records
                .iter_mut()
                .find(|row| row["event"] == event)
                .unwrap();
            match case {
                0 => row["selfSha256"] = json!("2".repeat(64)),
                1 => row["d1Qualified"] = json!(false),
                2 => row["qualified"] = json!(true),
                3 => row["d2Authority"] = json!(true),
                4 => row["releaseAuthority"] = json!(true),
                5 => row["actualTerminal"]["code"] = json!(1),
                6 => row["actualTerminal"]["signal"] = json!("SIGTERM"),
                7 => {
                    row.as_object_mut().unwrap().remove("actualTerminal");
                }
                8 => row["actualTerminal"]["extra"] = json!(true),
                9 => row["failure"] = json!("image_qualification_not_proven"),
                10 => row["originalFailure"] = json!("original_image_failure"),
                11 => row["failedArchiveRetained"] = json!(true),
                12 => row["elapsedMs"] = json!(-1),
                13 => row["elapsedMs"] = json!(1.5),
                14 => row["time"] = json!("unit-only"),
                15 => {
                    row.as_object_mut().unwrap().remove("failure");
                }
                16 => row["extra"] = json!(true),
                17 => {
                    row.as_object_mut().unwrap().remove("d1Qualified");
                }
                18 => {
                    row.as_object_mut().unwrap().remove("selfSha256");
                }
                19 => row["success"] = json!(false),
                20 => row["actualTerminal"] = Value::Null,
                21 => {
                    row["actualTerminal"]
                        .as_object_mut()
                        .unwrap()
                        .remove("signal");
                }
                22 => {
                    row.as_object_mut().unwrap().remove("originalFailure");
                }
                _ => row["elapsedMs"] = json!(9_007_199_254_740_992_u64),
            }
            // Recompute the journal/stdout/terminal chain. A valid outer digest
            // must not mask different controller bytes or failed inner claims.
            unit_rebind_controller_journal(&mut metadata, &records);
            let error =
                validate_measured_d1_controller(&metadata, &expected, &manifest, &qualification)
                    .unwrap_err();
            let code = match case {
                0 | 18 => "measured_d1_controller_source",
                7 | 15 | 16 | 17 | 22 => "measured_d1_closed_shape",
                19 => "measured_d1_journal_sequence",
                _ => "measured_d1_controller_finished",
            };
            assert_eq!(error.to_string(), code, "case {case}");
        }
    }

    #[test]
    fn measured_startup_expected_d1_preserves_terminal_cleanup_and_stage_denials() {
        let expected = unit_d1_expected();
        for case in 0..6 {
            let mut metadata = unit_d1_metadata();
            let manifest = d1_json(&metadata.export_manifest, 65_536, false).unwrap();
            let qualification = d1_json(&metadata.qualification, 65_536, false).unwrap();
            let mut records: Vec<Value> = std::str::from_utf8(&metadata.controller_journal)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            let event = [
                "command_terminal",
                "residue",
                "controller_finished",
                "source_after",
                "dispatch",
                "image_progress",
            ][case];
            let row = records
                .iter_mut()
                .find(|row| row["event"] == event)
                .unwrap();
            match case {
                0 => row["code"] = json!(1),
                1 => row["states"][0]["absent"] = json!(false),
                2 => row["success"] = json!(false),
                3 => row["manifest"] = json!("2".repeat(64)),
                4 => row["event"] = json!("unknown_stage"),
                _ => row["phase"] = json!("image_inspection_finished"),
            }
            // Rebind terminal hashes so the existing semantic guards, not merely
            // an outdated journal digest, must refuse the changed record family.
            unit_rebind_controller_journal(&mut metadata, &records);
            assert!(validate_measured_d1_controller(
                &metadata,
                &expected,
                &manifest,
                &qualification
            )
            .is_err());
        }
    }

    #[test]
    fn measured_startup_request_rejects_missing_packages_and_preclaimed_authority_fields() {
        let request = unit_startup_request();
        assert_eq!(request.operation, "issue_measured_runner_startup");
        for key in [
            "trust_policy",
            "release",
            "cohort",
            "activation",
            "startup_evidence_base64url",
            "release_test_evidence_base64url",
        ] {
            let mut value = unit_startup_request_json();
            value.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<MeasuredRunnerStartupRequest>(value).is_err());
        }
        for key in [
            "volume_seed",
            "volume_id",
            "claimed_instance",
            "account_id",
            "runtime_observed",
            "environment",
        ] {
            let mut value = unit_startup_request_json();
            value[key] = json!("forbidden");
            assert!(serde_json::from_value::<MeasuredRunnerStartupRequest>(value).is_err());
        }
        let binding =
            validate_measured_runner_d1(&unit_d1_metadata(), &unit_d1_expected(), "runner-641.0")
                .unwrap();
        let pool = super::super::tests::test_pool();
        assert!(issue_measured_runner_startup(&pool, &request, &binding).is_err()); // actual CLI requires guarded PG
        let count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM jobs_runner_volume_admission_grants",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn measured_startup_release_test_evidence_keeps_normal_v2_exact_tip_meaning() {
        let commit = "1".repeat(40);
        let value = json!({"version":2,"audience":"bluey-jobs-managed-cloud-test-evidence-v2","sourceCommit":commit,
            "repository":"unit-only/metadata","repositoryId":1,"defaultBranch":"main",
            "releaseContractSelfTestSha256":"a".repeat(64),"ciRuns":[
                {"checkId":"jobs-ci","workflowPath":".github/workflows/jobs-ci.yml","conclusion":"success",
                    "event":"push","headBranch":"main","headSha":commit,"repositoryId":1,"runAttempt":1,"runId":1},
                {"checkId":"observability-policy","workflowPath":".github/workflows/observability-policy.yml","conclusion":"success",
                    "event":"push","headBranch":"main","headSha":commit,"repositoryId":1,"runAttempt":1,"runId":2}]});
        validate_startup_release_test_evidence(&value, &commit).unwrap(); // unit metadata, not executed CI
        for index in 0..6 {
            let mut changed = value.clone();
            match index {
                0 => changed["sourceCommit"] = json!("2".repeat(40)),
                1 => changed["ciRuns"][0]["headSha"] = json!("2".repeat(40)),
                2 => changed["ciRuns"][0]["event"] = json!("pull_request"),
                3 => changed["ciRuns"][1]["runId"] = json!(1),
                4 => changed["ciRuns"][0]["conclusion"] = json!("failure"),
                _ => changed["d1_success"] = json!(true),
            }
            assert!(validate_startup_release_test_evidence(&changed, &commit).is_err());
        }
    }

    #[test]
    fn measured_startup_aggregate_binds_each_real_input_without_replacing_release_test_evidence() {
        use base64::Engine as _;
        let d1 =
            validate_measured_runner_d1(&unit_d1_metadata(), &unit_d1_expected(), "runner-641.0")
                .unwrap();
        let normal_test_digest = "a".repeat(64);
        let aggregate = json!({"version":1,"audience":"bluey-jobs-phase641-nonproduction-startup-only-evidence-v1",
            "sourceCommit":d1.source_commit,"releaseTestEvidenceSha256":normal_test_digest,
            "d1ExportStartedSha256":d1.export_started_sha256,"d1ExportManifestSha256":d1.export_manifest_sha256,
            "d1QualificationSha256":d1.qualification_sha256,"d1ControllerJournalSha256":d1.controller_journal_sha256,
            "d1ControllerFinalStdoutSha256":d1.controller_final_stdout_sha256,"d1ExternalTerminalSha256":d1.external_terminal_sha256});
        let encode = |value: &Value| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(managed_cloud_canonical_json(value).unwrap())
        };
        let digest =
            validate_startup_evidence_binding(&encode(&aggregate), &d1, &normal_test_digest)
                .unwrap();
        assert!(managed_cloud_token(
            &format!("phase641-startup-evidence-{digest}"),
            1024
        ));
        for key in aggregate.as_object().unwrap().keys() {
            let mut changed = aggregate.clone();
            changed.as_object_mut().unwrap().remove(key);
            assert!(
                validate_startup_evidence_binding(&encode(&changed), &d1, &normal_test_digest)
                    .is_err()
            );
        }
        for key in [
            "releaseTestEvidenceSha256",
            "d1ExternalTerminalSha256",
            "d1ControllerFinalStdoutSha256",
            "audience",
        ] {
            let mut changed = aggregate.clone();
            changed[key] = json!("wrong");
            assert!(
                validate_startup_evidence_binding(&encode(&changed), &d1, &normal_test_digest)
                    .is_err()
            );
        }
        let mut changed = aggregate;
        changed["caller_success"] = json!(true);
        assert!(
            validate_startup_evidence_binding(&encode(&changed), &d1, &normal_test_digest).is_err()
        );
    }

    #[test]
    fn measured_startup_header_rejects_scope_tokens_bounds_and_mixed_process_measurements() {
        let d1 =
            validate_measured_runner_d1(&unit_d1_metadata(), &unit_d1_expected(), "runner-641.0")
                .unwrap();
        let fresh = || {
            let mut request = unit_startup_request();
            request.process_runtime = d1.process_runtime.clone();
            request
        };
        validate_measured_startup_header(&fresh(), &d1).unwrap();
        for authorization in [false, true] {
            for valid in ["a".repeat(20), "a".repeat(128)] {
                let mut request = fresh();
                if authorization {
                    request.authorization_ref = valid;
                } else {
                    request.issuance_ref = valid;
                }
                validate_measured_startup_header(&request, &d1).unwrap();
            }
            for invalid in [
                "a".repeat(19),
                "a".repeat(129),
                format!("{}:", "a".repeat(19)),
                format!("{}.", "a".repeat(19)),
            ] {
                let mut request = fresh();
                if authorization {
                    request.authorization_ref = invalid;
                } else {
                    request.issuance_ref = invalid;
                }
                assert!(validate_measured_startup_header(&request, &d1).is_err());
            }
        }
        for index in 0..10 {
            let mut request = fresh();
            match index {
                0 => request.version = 2,
                1 => request.operation = "bootstrap".into(),
                2 => request.worker_id = "owner".into(),
                3 => request.issuance_ref = "invalid space".into(),
                4 => request.provider_resource_id = "production-volume".into(),
                5 => request.resource_fingerprint = "0".repeat(64),
                6 => request.ttl_ms = 4_999,
                7 => request.ttl_ms = 900_001,
                8 => request.process_runtime.runner_image_sha256 = "f".repeat(64),
                _ => request.process_runtime.automation_bundle_sha256 = "e".repeat(64),
            }
            assert!(validate_measured_startup_header(&request, &d1).is_err());
        }
    }

    #[test]
    fn measured_startup_unit_only_grant_helper_issues_three_without_new_claims_or_enrollment() {
        // Existing synthetic signed head is used ONLY here, with zero preclaims.
        // This is a grant-state unit test, never runtime/CI/storage/fleet proof.
        let pool = super::super::tests::test_pool();
        let now: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT CAST((julianday('now')-2440587.5)*86400000 AS INTEGER)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let (activation, manifest) =
            managed_cloud_release_authority_tests::install_valid_original_source_managed_head(
                &pool,
                3,
                now,
                300_000,
                24 * 60 * 60_000,
                fixture_scope(),
                "phase641-unit-only",
            );
        let (identity,artifact):(String,String)=pool.get().unwrap().query_row(
            "SELECT identity.runtime_identity_sha256,component.artifact_sha256
               FROM jobs_managed_cloud_manifest_runtime_identities identity
               JOIN jobs_managed_cloud_manifest_components component
                 ON component.manifest_sha256=identity.manifest_sha256 AND component.component_id=identity.component_id
              WHERE identity.manifest_sha256=?1 AND identity.component_id='jobs-runner' AND identity.role='managed_runner'",
            params![manifest],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
        let authority = MeasuredRunnerAuthority {
            manifest_sha256: manifest,
            activation_sha256: activation,
            runtime_identity_sha256: identity,
            expires_at_ms: now + 60_000,
        };
        let mut d1 =
            validate_measured_runner_d1(&unit_d1_metadata(), &unit_d1_expected(), "runner-641.0")
                .unwrap();
        d1.process_runtime.runner_image_sha256 = artifact; // unit head artifact, not an executed OCI image
        let mut request = unit_startup_request();
        request.process_runtime = d1.process_runtime.clone();
        let counts = || {
            let conn = pool.get().unwrap();
            [
                "jobs_managed_cloud_runtime_grants",
                "jobs_runner_volume_admission_grants",
                "jobs_runner_process_runtime_grants",
                "jobs_managed_cloud_runtime_instances",
                "jobs_runner_volumes",
                "jobs_runner_process_runtime_bindings",
                "jobs_runner_volume_storage_attestations",
                "jobs_applications",
                "jobs_runner_volume_residencies",
                "jobs_managed_cloud_irreversible_effect_receipts",
            ]
            .map(|table| {
                conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap()
            })
        };
        let before = counts();
        assert_eq!(before, [0; 10]);
        let issued = issue_startup_grants(&pool, &request, &authority, now, &d1).unwrap();
        let after = counts();
        assert_eq!(&after[..3], &[before[0] + 1, before[1] + 1, before[2] + 1]);
        assert_eq!(&after[3..], &before[3..]);
        assert_eq!((issued.grants_issued, issued.grants_consumed), (3, 0));
        assert!(
            !issued.runtime_observed
                && !issued.customer_admission_proven
                && !issued.release_authority
        );
        assert!(issued.volume_admission_grant.consumed_volume_id.is_none());
        assert!(issue_startup_grants(&pool, &request, &authority, now, &d1).is_err());
        assert_eq!(counts(), after);
        assert!(
            issue_startup_grants(&pool, &request, &authority, authority.expires_at_ms, &d1)
                .is_err()
        );
        assert_eq!(counts(), after);
    }

    fn checkpoint_application() -> JobApplication {
        JobApplication {
            id: "synthetic-application".into(),
            job_id: "synthetic-job".into(),
            resume_version_id: Some("synthetic-resume".into()),
            state: "running".into(),
            submission_mode: "review_first".into(),
            match_score: 93,
            answers: vec![json!({"question":"authorized","answer":true})],
            cover_letter: "synthetic cover letter".into(),
            receipt: json!({
                "application_identity": {"id":"synthetic-identity"},
                "packet": {"checksum":"packet-a","resume_version_id":"synthetic-resume"}
            }),
            run_id: Some("synthetic-run".into()),
            created_at_ms: 10,
            updated_at_ms: 20,
            submitted_at_ms: None,
        }
    }

    fn checkpoint_session() -> BrowserSession {
        BrowserSession {
            id: "cloud-synthetic-application".into(),
            runner: "cloud".into(),
            status: "running".into(),
            current_company: "Synthetic employer".into(),
            current_step: "Filling application".into(),
            application_id: Some("synthetic-application".into()),
            takeover_url: None,
            created_at_ms: 10,
            updated_at_ms: 20,
        }
    }

    #[test]
    fn checkpoint_inspection_schema_is_closed_to_exact_eight_fields() {
        let value = json!({"version":1,"operation":"inspect_checkpoint_recovery",
            "stage":"before_recovery","account_id":format!("ui_fixture_{}", uuid::Uuid::new_v4()),
            "application_id":"synthetic-application","run_id":"synthetic-run",
            "browser_session_id":"cloud-synthetic-application",
            "browser_profile_id":"synthetic-profile"});
        let request: RunnerControlPlaneInspectCheckpointRecoveryRequest =
            serde_json::from_value(value.clone()).unwrap();
        assert!(validate_checkpoint_inspection_request(&request).is_ok());
        assert_eq!(value.as_object().unwrap().len(), 8);
        for field in [
            "lease_token",
            "data_key",
            "candidate",
            "resume_content",
            "table",
        ] {
            let mut invalid = value.clone();
            invalid[field] = json!("forbidden");
            assert!(
                serde_json::from_value::<RunnerControlPlaneInspectCheckpointRecoveryRequest>(
                    invalid
                )
                .is_err()
            );
        }
        let mut wrong_stage = value;
        wrong_stage["stage"] = json!("recovered");
        assert!(
            serde_json::from_value::<RunnerControlPlaneInspectCheckpointRecoveryRequest>(
                wrong_stage
            )
            .is_err()
        );
    }

    #[test]
    fn checkpoint_stable_hashes_ignore_only_permitted_lifecycle_fields() {
        let application = checkpoint_application();
        let expected_application = stable_application_sha256(&application).unwrap();
        let mut recovered = application.clone();
        recovered.state = "failed".into();
        recovered.updated_at_ms = 30;
        recovered.receipt["cloud_recovery"] = json!({
            "schema_version":1,"status":"released","checkpoint_version":2,
            "checkpoint_phase":"prepared","lease_phase":"prepared",
            "recorded_at_ms":30,"run_id":"synthetic-run"
        });
        assert_eq!(
            stable_application_sha256(&recovered).unwrap(),
            expected_application
        );
        let mut answer_changed = application.clone();
        answer_changed.answers[0]["answer"] = json!(false);
        assert_ne!(
            stable_application_sha256(&answer_changed).unwrap(),
            expected_application
        );
        let mut packet_changed = application.clone();
        packet_changed.receipt["packet"]["checksum"] = json!("packet-b");
        assert_ne!(
            stable_application_sha256(&packet_changed).unwrap(),
            expected_application
        );
        let mut identity_changed = application.clone();
        identity_changed.receipt["application_identity"]["id"] = json!("other-identity");
        assert_ne!(
            stable_application_sha256(&identity_changed).unwrap(),
            expected_application
        );
        let mut submitted_changed = application;
        submitted_changed.submitted_at_ms = Some(31);
        assert_ne!(
            stable_application_sha256(&submitted_changed).unwrap(),
            expected_application
        );
        let mut run_changed = checkpoint_application();
        run_changed.run_id = Some("other-run".into());
        assert_ne!(
            stable_application_sha256(&run_changed).unwrap(),
            expected_application
        );
        let mut created_changed = checkpoint_application();
        created_changed.created_at_ms = 11;
        assert_ne!(
            stable_application_sha256(&created_changed).unwrap(),
            expected_application
        );

        let session = checkpoint_session();
        let expected_session = stable_session_sha256(&session).unwrap();
        let mut stopped = session.clone();
        stopped.status = "failed".into();
        stopped.current_step = "Browser run stopped before submission".into();
        stopped.updated_at_ms = 30;
        stopped.takeover_url = Some("https://not-exported.invalid".into());
        assert_eq!(stable_session_sha256(&stopped).unwrap(), expected_session);
        let mut rebound = session;
        rebound.application_id = Some("other-application".into());
        assert_ne!(stable_session_sha256(&rebound).unwrap(), expected_session);
    }

    #[test]
    fn checkpoint_cloud_recovery_requires_exact_closed_seven_field_summary() {
        let value = json!({"schema_version":1,"status":"released","checkpoint_version":2,
            "checkpoint_phase":"prepared","lease_phase":"prepared","recorded_at_ms":30,
            "run_id":"synthetic-run"});
        assert_eq!(value.as_object().unwrap().len(), 7);
        assert!(
            serde_json::from_value::<RunnerControlPlaneCloudRecoverySummary>(value.clone()).is_ok()
        );
        let mut invalid = value;
        invalid["lease_token"] = json!("forbidden");
        assert!(serde_json::from_value::<RunnerControlPlaneCloudRecoverySummary>(invalid).is_err());
    }

    fn inspection_request() -> RunnerControlPlaneInspectAutoSubmitClaimRequest {
        RunnerControlPlaneInspectAutoSubmitClaimRequest {
            version: 1,
            operation: "inspect_auto_submit_claim".into(),
            account_id: format!("ui_fixture_{}", uuid::Uuid::new_v4()),
            application_id: "synthetic-application".into(),
            run_id: "synthetic-run".into(),
            browser_session_id: "cloud-synthetic-application".into(),
            browser_profile_id: "synthetic-profile".into(),
            lease_token_sha256: "a".repeat(64),
            lease_fence: 1,
        }
    }

    #[test]
    fn auto_inspection_requires_closed_operation_hash_fence_and_distinct_session() {
        assert!(validate_inspect_request(&inspection_request()).is_ok());
        for change in 0..8 {
            let mut request = inspection_request();
            match change {
                0 => request.version = 2,
                1 => request.operation = "post_queue_auto_submit".into(),
                2 => request.account_id = "owner".into(),
                3 => request.browser_session_id = request.run_id.clone(),
                4 => request.lease_token_sha256 = "A".repeat(64),
                5 => request.lease_token_sha256 = "a".repeat(63),
                6 => request.lease_fence = 0,
                7 => request.lease_fence = 9_007_199_254_740_992,
                _ => unreachable!(),
            }
            assert!(validate_inspect_request(&request).is_err());
        }
        let value = json!({"version":1,"operation":"inspect_auto_submit_claim",
            "account_id":"a","application_id":"p","run_id":"r",
            "browser_session_id":"s","browser_profile_id":"b",
            "lease_token_sha256":"a".repeat(64),"lease_fence":1});
        assert!(
            serde_json::from_value::<RunnerControlPlaneInspectAutoSubmitClaimRequest>(
                value.clone()
            )
            .is_ok()
        );
        for extra in ["lease_token", "claim", "environment", "frozen_authority"] {
            let mut invalid = value.clone();
            invalid[extra] = json!("forbidden");
            assert!(
                serde_json::from_value::<RunnerControlPlaneInspectAutoSubmitClaimRequest>(invalid)
                    .is_err()
            );
        }
    }

    #[test]
    fn review_first_handoff_wire_round_trip_omits_auto_summary() {
        let h = "a".repeat(64);
        let original = json!({"version":1,"operation":"post_queue","account_id":"a",
            "application_id":"p","run_id":"r","browser_session_id":"s",
            "browser_profile_id":"b","command_id":"c","request_id":"q","workflow_id":"w",
            "submission_mode":"review_first","managed_cloud_release":{
                "version":1,"bindingSha256":h,"scope":fixture_scope(),"headRevision":1,
                "transitionSha256":h,"activationSha256":h,"manifestSha256":h,"cohortSha256":h,
                "trustGeneration":1,"channelSequence":1,"releaseId":"synthetic-release",
                "releaseSequence":1,"taskQueueSha256":h,"failureConverterSha256":h,
                "readinessSha256":h,"activationExpiresAtMs":2,"resolvedAtMs":1}});
        let result: RunnerControlPlanePostQueueResult =
            serde_json::from_value(original.clone()).unwrap();
        assert!(result.auto_summary.is_none());
        assert_eq!(serde_json::to_value(result).unwrap(), original);
        assert_eq!(FixtureMode::ReviewFirst.bootstrap_operation(), "bootstrap");
        assert_eq!(
            FixtureMode::ReviewFirst.post_queue_operation(),
            "post_queue"
        );
        assert_eq!(FixtureMode::ReviewFirst.submission_mode(), "review_first");
        assert_eq!(
            FixtureMode::AutoSubmit.bootstrap_operation(),
            "bootstrap_auto_submit"
        );
        assert_eq!(
            FixtureMode::AutoSubmit.post_queue_operation(),
            "post_queue_auto_submit"
        );
        assert_eq!(FixtureMode::AutoSubmit.submission_mode(), "auto_submit");
    }

    #[test]
    fn auto_summary_is_closed_and_has_no_missing_field_fallback() {
        let value = json!({"attempt_id":"attempt","auto_authorization_id":"authorization",
            "auto_authorization_revision":1,"auto_authorization_fingerprint_sha256":"a".repeat(64),
            "packet_checksum_sha256":"b".repeat(64),"provider":"greenhouse","target_key_sha256":"c".repeat(64),
            "manifest_sha256":"d".repeat(64),"activation_sha256":"e".repeat(64),"layout_set_sha256":"f".repeat(64),
            "adapter_bundle_sha256":"1".repeat(64),"runner_target_sha256":"2".repeat(64)});
        assert!(serde_json::from_value::<RunnerControlPlaneAutoSummary>(value.clone()).is_ok());
        for field in value.as_object().unwrap().keys() {
            let mut invalid = value.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<RunnerControlPlaneAutoSummary>(invalid).is_err());
        }
        let mut invalid = value;
        invalid["resume_content"] = json!("must not be exported");
        assert!(serde_json::from_value::<RunnerControlPlaneAutoSummary>(invalid).is_err());
    }

    #[test]
    fn synthetic_runtime_uses_existing_canonical_build_and_ats_contracts() {
        let runtime = runtime_model("synthetic-unit-only");
        validate_runner_process_runtime(&runtime).expect("valid protocol runtime metadata");
        assert!(runner_build_id_is_canonical(&runtime.runner_build_id));
        let target = runtime_target(&runtime).expect("derive runtime target");
        validate_ats_runtime_target(&target).expect("valid cloud ATS target");
        assert_eq!(target.runtime_kind, "cloud");
        assert!(target.browser_release_manifest_sha256.is_none());
        assert_eq!(fixture_scope().environment, "staging");
    }

    #[test]
    fn synthetic_account_requires_exact_generated_uuid_namespace() {
        let id = uuid::Uuid::new_v4().to_string();
        assert!(synthetic_account_id(&format!("ui_fixture_{id}")));
        for value in [
            id,
            "ui_fixture_owner".into(),
            "owner".into(),
            "ui_fixture_00000000-0000-0000-0000-000000000000".into(),
        ] {
            assert!(!synthetic_account_id(&value));
        }
    }
}
