import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import {
  artifactAttachmentPlan,
  completeImageWorkload,
  preserveFailedArchive,
  validateFailedArchiveDescriptor,
  buildArguments,
  captureBuildFailureDiagnostic,
  cleanupManagedRunnerImageResources,
  dockerEnvironment,
  emitWorkloadProgress,
  flushStagedImageEvidence,
  parseBuildkitDaemonVersion,
  parseOciArchiveIndex,
  parseOciConfigDescriptor,
  parseOciRuntimeShapeEvidence,
  parseSelectedGitTree,
  selectedPaths,
  selectBuilderContainerMetadata,
  selectBuilderImageMetadata,
  selectBuiltImageMetadata,
  selectEngineMetadata,
  selectSmokeContainerMetadata,
  validateDiskSpace,
  validateStorageMounts,
  validateContainerdEngine,
  validateExpectedSourceHead,
  validateBuildkitManifestIdentity,
  validateImageInspectionReport,
  validatePrivateDirectoryStats,
  validateLoadedManifestIdentity,
  validateQualification,
  validateSmokeContainerInspection,
} from "./managed-runner-image-workload.mjs";

const sourceCommit = "a".repeat(40);
const invalidSourceCommits = Object.freeze([
  undefined,
  "",
  "0".repeat(40),
  sourceCommit.toUpperCase(),
  "g".repeat(40),
  sourceCommit.slice(1),
  sourceCommit + "\n",
  sourceCommit + "\r\n",
]);
const expected = () => ({ version: 1, sourceCommit,
  buildId: `nonprod-p639-${sourceCommit.slice(0, 12)}`, configSchemaSha256: "1".repeat(64),
  migrationSetSha256: "2".repeat(64), protocolSetSha256: "3".repeat(64) });
const report = () => ({
  version: 1, expected: expected(),
  oci: { manifestSha256: "4".repeat(64), imageConfigSha256: "5".repeat(64),
    inventorySha256: "6".repeat(64) },
  runtime: { runtimeMeasurementSha256: "7".repeat(64),
    runtimeIdentitySha256: "8".repeat(64), automationBundleSha256: "9".repeat(64) },
  native: { writeReadback: true, inventory: true, removed: true },
  browser: {
    executablePath:
      "/ms-playwright/chromium_headless_shell-1228/chrome-linux/chrome-headless-shell",
    executableSha256: "a".repeat(64), chromiumRevision: "1228",
    playwrightVersion: "1.61.1", chromiumVersion: "148.0.1.2",
    launchProfile: "production_default_options_synthetic_only",
    titleReadback: true, requestsAborted: 0,
  },
  claims: { actualImageInspection: true, nativeAddon: true, browserOnly: true,
    normalRunnerStartup: false, managedAuthority: false, submission: false, production: false },
  inspectorSha256: "b".repeat(64),
});
const treeRecord = (path, mode = "100644", digest = "a".repeat(40)) =>
  `${mode} blob ${digest}\t${path}\0`;

test("default selected tree count is exactly the reviewed 797-file integration", () => {
  const tree = (count) => Buffer.from(Array.from({ length: count }, (_, index) =>
    treeRecord(`jobs/fixture-${String(index).padStart(4, "0")}`)).join(""));
  assert.equal(parseSelectedGitTree(tree(797)).count, 797);
  for (const count of [778, 796, 798]) {
    assert.throws(() => parseSelectedGitTree(tree(count)), /source_tree_count/);
  }
});

test("selected Git tree admits canonical dotfiles but denies secret and ambiguous paths", () => {
  const bytes = Buffer.from(treeRecord(".github/workflows/jobs.yml")
    + treeRecord("jobs/.dockerignore") + treeRecord("jobs/.env.example")
    + treeRecord("jobs/runner/Dockerfile", "100755"));
  assert.deepEqual(parseSelectedGitTree(bytes, 4).paths,
    [".github/workflows/jobs.yml", "jobs/.dockerignore", "jobs/.env.example",
      "jobs/runner/Dockerfile"]);
  for (const path of ["jobs/.env", "jobs/config/credentials.json", "jobs/id_rsa",
    "jobs/private-key.pem", "jobs/../server/escape", "/jobs/escape", "jobs\\escape"]) {
    assert.throws(() => parseSelectedGitTree(Buffer.from(treeRecord(path)), 1));
  }
  assert.throws(() => parseSelectedGitTree(Buffer.from(treeRecord("jobs/a", "120000")), 1));
  assert.throws(() => parseSelectedGitTree(Buffer.from(
    treeRecord("jobs/b") + treeRecord("jobs/a")), 2));
  assert(selectedPaths.includes(".github") && selectedPaths.includes("jobs"));
});

test("build command is one closed amd64 build with six nonzero authority arguments", () => {
  const args = buildArguments(expected(), 1761955200);
  assert.equal(args.filter((item) => item === "--build-arg").length, 6);
  assert.equal(args.filter((item) => item === "build").length, 1);
  assert(args.includes("--builder") && args.includes("p639-image"));
  assert(args.includes("linux/amd64") && !args.includes("--load") && !args.includes("--tag"));
  assert(args.includes("type=oci,dest=/var/tmp/bluey-jobs-runner-image-p639/fixture/"
    + "runner.oci.tar,compression=uncompressed,force-compression=true"));
  assert.equal(args.includes("--use"), false);
  assert.throws(() => buildArguments({ ...expected(), protocolSetSha256: "0".repeat(64) },
    1761955200));
  assert.throws(() => buildArguments(expected(), 1));
  assert.deepEqual(dockerEnvironment().slice(0, 2), ["env", "-i"]);
  assert(dockerEnvironment().includes(
    "DOCKER_CONFIG=/var/tmp/bluey-jobs-runner-image-p639/docker-config"));
});

test("BuildKit daemon version is an exact single primary-format line", () => {
  const revision = "a".repeat(40);
  assert.deepEqual(parseBuildkitDaemonVersion(
    `buildkitd github.com/moby/buildkit v0.34.1 ${revision}\n`),
  { version: "v0.34.1", revision });
  for (const text of ["", "BuildKit version: v0.34.1\n",
    `buildkitd github.com/moby/buildkit v0.34.1 ${revision}`,
    `buildkitd github.com/moby/buildkit v0.34.1 ${revision}\nextra\n`,
    `buildkitd github.com/moby/buildkit v0.34.0 ${revision}\n`,
    `buildkitd github.com/moby/buildkit v0.34.10 ${revision}\n`,
    `buildkitd github.com/moby/buildkit v0.34.1 ${"A".repeat(40)}\n`,
    `buildkitd github.com/moby/buildkit v0.34.1 unknown\n`]) {
    assert.throws(() => parseBuildkitDaemonVersion(text));
  }
});

test("OCI index and Docker containerd identity bind only the manifest digest", () => {
  const manifest = "c".repeat(64);
  const index = parseOciArchiveIndex(JSON.stringify({
    schemaVersion: 2,
    mediaType: "application/vnd.oci.image.index.v1+json",
    annotations: { "org.opencontainers.image.created": "2026-11-01T00:00:00Z" },
    manifests: [{
      mediaType: "application/vnd.oci.image.manifest.v1+json",
      digest: `sha256:${manifest}`,
      size: 2261,
      platform: { os: "linux", architecture: "amd64" },
      annotations: { "org.opencontainers.image.ref.name": "synthetic" },
    }],
  }));
  assert.equal(index.manifestSha256, manifest);
  const image = { Id: index.digest, Descriptor: { digest: index.digest,
    mediaType: index.mediaType, size: index.size,
    platform: { os: "linux", architecture: "amd64" } } };
  assert.equal(validateLoadedManifestIdentity(image, index), image);
  for (const mutate of [
    (value) => { value.Id = `sha256:${"5".repeat(64)}`; },
    (value) => { value.Descriptor.digest = `sha256:${"d".repeat(64)}`; },
    (value) => { value.Descriptor.mediaType = "application/vnd.docker.distribution.manifest.v2+json"; },
    (value) => { value.Descriptor.size += 1; },
    (value) => { value.Descriptor.platform.architecture = "arm64"; },
  ]) {
    const changed = structuredClone(image);
    mutate(changed);
    assert.throws(() => validateLoadedManifestIdentity(changed, index));
  }
  assert.throws(() => parseOciArchiveIndex(JSON.stringify({ schemaVersion: 2,
    manifests: [{}, {}] })));
  assert.throws(() => parseOciArchiveIndex(JSON.stringify({ schemaVersion: 2,
    manifests: [{ mediaType: "application/vnd.oci.image.manifest.v1+json",
      digest: `sha256:${manifest}`, size: 2261,
      platform: { os: "linux", architecture: "arm64" } }] })));
});

function configDiagnosticFixture(runtime) {
  const hash = (text) => createHash("sha256").update(text).digest("hex");
  const configText = JSON.stringify({ architecture: "amd64", os: "linux", config: runtime });
  const config = { mediaType: "application/vnd.oci.image.config.v1+json",
    digest: `sha256:${hash(configText)}`, size: Buffer.byteLength(configText) };
  const manifestText = JSON.stringify({ schemaVersion: 2,
    mediaType: "application/vnd.oci.image.manifest.v1+json", config, layers: [] });
  return { configText, manifestText,
    index: { manifestSha256: hash(manifestText), size: Buffer.byteLength(manifestText) },
    descriptor: { digest: config.digest, sha256: config.digest.slice(7), size: config.size } };
}

test("OCI config diagnostics bind exact blobs and expose only fixed key metadata", () => {
  const runtime = { ArgsEscaped: true, Env: ["SECRET=PRIVATE_VALUE"],
    User: "PRIVATE_USER", Labels: { private: "PRIVATE_LABEL" },
    PRIVATE_UNKNOWN_KEY: "PRIVATE_UNKNOWN_VALUE" };
  const fixture = configDiagnosticFixture(runtime);
  const descriptor = parseOciConfigDescriptor(fixture.manifestText, fixture.index);
  assert.deepEqual(descriptor, fixture.descriptor);
  const evidence = parseOciRuntimeShapeEvidence(fixture.configText, descriptor);
  assert.equal(evidence.configSha256, descriptor.sha256);
  assert.equal(evidence.configBytes, descriptor.size);
  assert.equal(evidence.runtimeKeyCount, 5);
  assert.equal(evidence.unknownRuntimeKeyCount, 1);
  assert.deepEqual(evidence.knownRuntimeKeys, ["ArgsEscaped", "Env", "Labels", "User"]);
  assert.equal(evidence.argsEscapedState, "true");
  assert.equal(evidence.envState, "array");
  assert.equal(evidence.envEntryCount, 1);
  assert.deepEqual(evidence.envOrderedKnownNames, ["unknown_or_invalid"]);
  assert.equal(evidence.envUnknownOrInvalid, true);
  assert.match(evidence.envSha256, /^[a-f0-9]{64}$/);
  assert.equal(evidence.runtimeKeySetSha256,
    createHash("sha256").update(JSON.stringify(Object.keys(runtime).sort()) + "\n").digest("hex"));
  assert(!JSON.stringify(evidence).includes("PRIVATE"));
  assert(!JSON.stringify(evidence).includes("SECRET"));
  assert(Object.isFrozen(evidence) && Object.isFrozen(evidence.knownRuntimeKeys));
});

test("OCI Env diagnostics redact values while preserving ordered allowed-name shape", () => {
  const env = ["PATH=/private", "BLUEY_JOBS_RUNNER_DATA=/private", "NODE_ENV=production",
    "PLAYWRIGHT_BROWSERS_PATH=/private"];
  const fixture = configDiagnosticFixture({ Env: env });
  const evidence = parseOciRuntimeShapeEvidence(fixture.configText, fixture.descriptor);
  assert.deepEqual(evidence.envOrderedKnownNames, ["PATH", "BLUEY_JOBS_RUNNER_DATA",
    "NODE_ENV", "PLAYWRIGHT_BROWSERS_PATH"]);
  assert.equal(evidence.envUnknownOrInvalid, false);
  assert.equal(evidence.envSha256, createHash("sha256").update(JSON.stringify(env) + "\n").digest("hex"));
  assert(!JSON.stringify(evidence).includes("private"));
  for (const [value, state] of [[{}, "absent"], [{ Env: null }, "other"], [{ Env: "PATH=x" }, "other"]]) {
    const fixture = configDiagnosticFixture(value);
    const evidence = parseOciRuntimeShapeEvidence(fixture.configText, fixture.descriptor);
    assert.equal(evidence.envState, state);
    assert.equal(evidence.envEntryCount, null);
    assert.equal(evidence.envSha256, null);
    assert.deepEqual(evidence.envOrderedKnownNames, []);
  }
  for (const Env of [[], Array(64).fill("PATH=x"), ["PATH=" + "x".repeat(4091)],
    ["PATH=" + "é".repeat(2045) + "x"]]) {
    const fixture = configDiagnosticFixture({ Env });
    const evidence = parseOciRuntimeShapeEvidence(fixture.configText, fixture.descriptor);
    assert.equal(evidence.envEntryCount, Env.length);
    assert(Object.isFrozen(evidence.envOrderedKnownNames));
    assert.equal(evidence.envSha256, createHash("sha256").update(JSON.stringify(Env) + "\n").digest("hex"));
  }
  for (const value of [{ Env: ["SECRET=x"] }, { Env: ["PATHx"] }, { Env: ["=x"] },
    { Env: ["LD_PRELOAD=x"] }, { Env: ["PATH=x", 1] }, { Env: Array(65).fill("PATH=x") },
    { Env: ["PATH=" + "x".repeat(4097)] }, { Env: ["PATH=" + "é".repeat(2046)] }]) {
    if (Array.isArray(value.Env) && value.Env.every((item) => typeof item === "string")
      && value.Env.length <= 64 && Buffer.byteLength(value.Env[0]) <= 4096) {
      const fixture = configDiagnosticFixture(value);
      const evidence = parseOciRuntimeShapeEvidence(fixture.configText, fixture.descriptor);
      assert.equal(evidence.envUnknownOrInvalid, true);
      assert.deepEqual(evidence.envOrderedKnownNames, ["unknown_or_invalid"]);
    } else assert.throws(() => parseOciRuntimeShapeEvidence(configDiagnosticFixture(value).configText,
      configDiagnosticFixture(value).descriptor));
  }
});

test("OCI config diagnostics distinguish absent and invalid compatibility values without admission", () => {
  for (const [runtime, state] of [[{}, "absent"], [{ ArgsEscaped: true }, "true"],
    [{ ArgsEscaped: false }, "false"], [{ ArgsEscaped: null }, "other"],
    [{ ArgsEscaped: "PRIVATE" }, "other"], [{ ArgsEscaped: {} }, "other"]]) {
    const fixture = configDiagnosticFixture(runtime);
    const value = parseOciRuntimeShapeEvidence(fixture.configText, fixture.descriptor);
    assert.equal(value.argsEscapedState, state);
    assert(!JSON.stringify(value).includes("PRIVATE"));
  }
});

test("OCI config diagnostic size digest JSON and key bounds reject tampering", () => {
  const fixture = configDiagnosticFixture({ ArgsEscaped: true });
  assert.throws(() => parseOciConfigDescriptor(fixture.manifestText + " ", fixture.index));
  assert.throws(() => parseOciConfigDescriptor(fixture.manifestText,
    { ...fixture.index, manifestSha256: "f".repeat(64) }));
  for (const descriptor of [{ ...fixture.descriptor, size: fixture.descriptor.size + 1 },
    { ...fixture.descriptor, sha256: "f".repeat(64) }]) {
    assert.throws(() => parseOciRuntimeShapeEvidence(fixture.configText, descriptor));
  }
  for (const runtime of [null, [], Object.fromEntries(Array.from({ length: 65 },
    (_, index) => [`key${index}`, null])), { ["🔑".repeat(33)]: true },
    { Env: ["x".repeat(128 * 1024)] }]) {
    const changed = configDiagnosticFixture(runtime);
    assert.throws(() => parseOciRuntimeShapeEvidence(changed.configText, changed.descriptor));
  }
  for (const config of [{ mediaType: "wrong", digest: "sha256:" + "a".repeat(64), size: 1 },
    { mediaType: "application/vnd.oci.image.config.v1+json", digest: "sha256:../../escape", size: 1 },
    { mediaType: "application/vnd.oci.image.config.v1+json", digest: "sha256:" + "a".repeat(64),
      size: 128 * 1024 + 1 }]) {
    const manifestText = JSON.stringify({ schemaVersion: 2,
      mediaType: "application/vnd.oci.image.manifest.v1+json", config });
    assert.throws(() => parseOciConfigDescriptor(manifestText, {
      size: Buffer.byteLength(manifestText),
      manifestSha256: createHash("sha256").update(manifestText).digest("hex"),
    }));
  }
});

test("exact OCI config shape evidence is staged before image load and inspection", async () => {
  const source = await readFile(new URL("./managed-runner-image-workload.mjs", import.meta.url), "utf8");
  const shape = source.indexOf('stage: "oci_runtime_config_shape"');
  const load = source.indexOf('"image", "load", "--input"');
  const inspection = source.indexOf('await emitWorkloadProgress(progress, "image_inspection_started")');
  assert(shape > 0 && load > shape && inspection > load);
});

test("BuildKit pin binds exact manifest, descriptor, repository digest, and running container", () => {
  const digest = "sha256:b651bf250c2805569f1db385ad1ea8fb73737dfc851de305f274ca02690fa93f";
  const mediaType = "application/vnd.oci.image.manifest.v1+json";
  const pin = `moby/buildkit@${digest}`;
  const image = { Id: digest, Descriptor: { digest, mediaType, size: 2261 },
    RepoDigests: [pin] };
  const container = { Image: digest,
    ImageManifestDescriptor: { digest, mediaType, size: 2261 },
    State: { Running: true }, Config: { Image: pin } };
  assert.doesNotThrow(() => validateBuildkitManifestIdentity(image, container));
  for (const mutate of [
    (value) => { value.image.Id = `sha256:${"2".repeat(64)}`; },
    (value) => { value.image.Descriptor.digest = `sha256:${"2".repeat(64)}`; },
    (value) => { value.image.RepoDigests.push("extra@sha256:x"); },
    (value) => { value.container.Image = `sha256:${"2".repeat(64)}`; },
    (value) => { value.container.ImageManifestDescriptor.size = 2262; },
    (value) => { value.container.State.Running = false; },
    (value) => { value.container.Config.Image = "moby/buildkit:latest"; },
  ]) {
    const value = { image: structuredClone(image), container: structuredClone(container) };
    mutate(value);
    assert.throws(() => validateBuildkitManifestIdentity(value.image, value.container));
  }
});

test("progress callback admits only the six closed phases in order", async () => {
  const phases = ["inputs_prepared", "builder_started", "image_build_started",
    "image_build_finished", "image_inspection_started", "image_inspection_finished"];
  const observed = [];
  for (const phase of phases) await emitWorkloadProgress(
    async (value) => observed.push(value), phase);
  assert.deepEqual(observed, phases);
  await assert.rejects(() => emitWorkloadProgress(async () => {}, "unknown"),
    /workload_progress_phase/);
});

test("selected Docker evidence excludes environment and preserves denial fields", async () => {
  const engine = selectEngineMetadata({ ServerVersion: "29.1.2", Driver: "overlay2",
    DriverStatus: [["Backing Filesystem", "extfs"]], Os: "linux", Architecture: "x86_64" });
  assert.equal(engine.ServerVersion, "29.1.2");
  const builderImage = selectBuilderImageMetadata({ Id: `sha256:${"1".repeat(64)}`,
    Descriptor: { digest: `sha256:${"2".repeat(64)}` }, RepoDigests: ["image@sha256:x"],
    RootFS: { Type: "layers", Layers: [] }, Config: { User: "root", Env: ["SECRET=x"] } });
  assert.deepEqual(Object.keys(builderImage.Config), ["User"]);
  const builderContainer = selectBuilderContainerMetadata({ Image: `sha256:${"1".repeat(64)}`,
    ImageManifestDescriptor: { digest: `sha256:${"2".repeat(64)}` },
    State: { Status: "running", Running: true, ExitCode: 0, Error: "" },
    Config: { Image: "pinned", Env: ["SECRET=x"] } });
  assert.deepEqual(builderContainer.State,
    { Status: "running", Running: true, ExitCode: 0, Error: "" });
  assert.deepEqual(Object.keys(builderContainer.Config), ["Image"]);
  const built = selectBuiltImageMetadata({ Id: `sha256:${"3".repeat(64)}`, Descriptor: null,
    RepoDigests: [], Os: "linux", Architecture: "amd64",
    Config: { User: "pwuser", Env: ["SECRET=x"] } });
  assert.deepEqual(Object.keys(built.Config), ["User"]);
  const smoke = selectSmokeContainerMetadata({ ...containerInspection(),
    Config: { Image: `sha256:${"3".repeat(64)}`, User: "pwuser", Env: ["SECRET=x"] } });
  assert.deepEqual(Object.keys(smoke.Config), ["Image", "User"]);
  assert.equal(JSON.stringify({ engine, builderImage, builderContainer, built, smoke })
    .includes("SECRET"), false);
  let captured;
  const flushed = await flushStagedImageEvidence(async (value) => { captured = value; },
    [{ stage: "engine_metadata", value: engine }]);
  assert.match(captured, /^Managed runner image workload evidence:\n/);
  assert(flushed.bytes > 0 && flushed.bytes <= 64 * 1024);
  await assert.rejects(() => flushStagedImageEvidence(async () => {}, []),
    /staged_evidence_shape/);
  assert.throws(() => selectEngineMetadata({ ...engine, DriverStatus: null }));
});

test("source retains bootstrap timeout, evidence ordering, and exact identity denial", async () => {
  const source = await readFile(new URL("./managed-runner-image-workload.mjs", import.meta.url),
    "utf8");
  assert(source.includes('BUILDER, "--bootstrap", "--timeout", "120s"'));
  const imageEvidence = source.indexOf('stage: "builder_image_metadata"');
  const containerEvidence = source.indexOf('stage: "builder_container_metadata"');
  const identityGate = source.indexOf(
    "validateBuildkitManifestIdentity(buildkitImage, buildkitContainer)");
  assert(imageEvidence > 0 && containerEvidence > imageEvidence && identityGate > containerEvidence);
  assert(source.includes("container.State.Running === true"));
  assert(!source.includes('buildkitContainer.Image === `sha256:${BUILDKIT_CONFIG_SHA256}`'));
});

test("disk preflight requires independent root and shared Docker/containerd capacity", () => {
  const header = "Filesystem 1024-blocks Used Available Capacity Mounted on\n";
  const rows = "/dev/root 7000000 1 5000000 1% /\n"
    + "/dev/data 16000000 1 13000000 1% /var/lib/docker\n"
    + "/dev/data 16000000 1 13000000 1% /var/lib/containerd\n";
  assert.deepEqual(validateDiskSpace(header + rows),
    { rootFreeBytes: 5_000_000 * 1024, dataFreeBytes: 13_000_000 * 1024,
      containerdFreeBytes: 13_000_000 * 1024 });
  for (const bad of [
    rows.replace("5000000", "100"), rows.replace("/dev/root", "/dev/data"),
    rows.replace("1% /\n", "1% /wrong\n"),
    rows.replace("/dev/data 16000000 1 13000000 1% /var/lib/containerd",
      "/dev/other 16000000 1 13000000 1% /var/lib/containerd"),
    rows.replace("16000000 1 13000000 1% /var/lib/containerd",
      "15000000 1 13000000 1% /var/lib/containerd"),
    rows.replace("13000000 1% /var/lib/containerd", "100 1% /var/lib/containerd"),
    rows.replace("7000000", "20000000"), rows.replace("5000000", "NaN"),
  ]) assert.throws(() => validateDiskSpace(header + bad));
  assert.equal(validatePrivateDirectoryStats(
    "501:20:700:directory\n501:20:700:directory\n", 2), true);
  assert.throws(() => validatePrivateDirectoryStats(
    "501:20:700:directory\n502:20:700:directory\n", 2));
});

test("storage mounts bind both sibling stores to the independent data device", () => {
  const mounts = [
    { source: "/dev/vda1", target: "/", fstype: "ext4", fsroot: "/", "maj:min": "8:1" },
    { source: "/dev/vdb[/docker]", target: "/var/lib/docker", fstype: "ext4",
      fsroot: "/docker", "maj:min": "8:16" },
    { source: "/dev/vdb[/containerd]", target: "/var/lib/containerd", fstype: "ext4",
      fsroot: "/containerd", "maj:min": "8:16" },
  ];
  const encode = (values) => values.map((row) => JSON.stringify({ filesystems: [row] }));
  assert.deepEqual(validateStorageMounts(encode(mounts)), mounts);
  for (const mutate of [
    (v) => { v[2].target = "/"; },
    (v) => { v[2]["maj:min"] = "8:1"; },
    (v) => { v[0]["maj:min"] = "8:16"; },
    (v) => { v[2].source = "/dev/other[/containerd]"; },
    (v) => { v[2].fsroot = "/docker"; },
    (v) => { v[2].fstype = "tmpfs"; },
    (v) => { v[2].children = []; },
  ]) { const value = structuredClone(mounts); mutate(value);
    assert.throws(() => validateStorageMounts(encode(value))); }
  assert.throws(() => validateStorageMounts(["{}", "{}", "{}"]));
  assert.throws(() => validateStorageMounts(["x".repeat(8193), "{}", "{}"]));
});

test("manifest protocol admits only the observed Docker29 containerd engine family", () => {
  const value = { Architecture: "x86_64", Driver: "overlayfs",
    DriverStatus: [["driver-type", "io.containerd.snapshotter.v1"]],
    Os: "linux", ServerVersion: "29.5.2" };
  assert.equal(validateContainerdEngine(value), value);
  for (const patch of [{ ServerVersion: "28.5.2" }, { Driver: "overlay2" },
    { DriverStatus: [] }, { Architecture: "aarch64" }, { Os: "darwin" }]) {
    assert.throws(() => validateContainerdEngine({ ...value, ...patch }));
  }
});

function containerInspection() {
  const options = (size) =>
    `rw,noexec,nosuid,nodev,size=${size},uid=1000,gid=1000,mode=0700`;
  return {
    Image: `sha256:${"4".repeat(64)}`,
    ImageManifestDescriptor: { digest: `sha256:${"4".repeat(64)}`,
      mediaType: "application/vnd.oci.image.manifest.v1+json", size: 2261 },
    State: { Status: "exited", ExitCode: 0 },
    HostConfig: { NetworkMode: "none", ReadonlyRootfs: true, PublishAllPorts: false,
      Privileged: false, CapAdd: null, IpcMode: "private", PidMode: "",
      Devices: [], DeviceRequests: null, DeviceCgroupRules: [],
      PortBindings: {}, PidsLimit: 256, Memory: 2 * 1024 ** 3, NanoCpus: 2_000_000_000,
      CapDrop: ["ALL"], SecurityOpt: ["no-new-privileges:true"],
      Tmpfs: { "/tmp": options(512 * 1024 * 1024),
        "/dev/shm": options(512 * 1024 * 1024),
        "/var/lib/bluey-jobs-runner": options(128 * 1024 * 1024) } },
    Mounts: [
      { Type: "bind", Source: "/var/tmp/root/out", Destination: "/out", RW: true },
      { Type: "bind", Source: "/var/tmp/root/fixture", Destination: "/fixture", RW: false },
    ],
  };
}

test("smoke inspection binds exact image, isolation, mounts, and private tmpfs", () => {
  const expectedContainer = { imageManifestId: `sha256:${"4".repeat(64)}`,
    imageManifestDescriptor: { digest: `sha256:${"4".repeat(64)}`,
      mediaType: "application/vnd.oci.image.manifest.v1+json", size: 2261 },
    fixture: "/var/tmp/root/fixture", out: "/var/tmp/root/out", uid: 1000, gid: 1000 };
  assert.equal(validateSmokeContainerInspection(containerInspection(), expectedContainer)
    .State.ExitCode, 0);
  for (const mutate of [
    (value) => { value.HostConfig.NetworkMode = "bridge"; },
    (value) => { value.HostConfig.PortBindings = { "8091/tcp": [{ HostPort: "8091" }] }; },
    (value) => { value.HostConfig.SecurityOpt = []; },
    (value) => { value.HostConfig.SecurityOpt = ["no-new-privileges:false"]; },
    (value) => { value.HostConfig.SecurityOpt = ["seccomp=unconfined"]; },
    (value) => { value.HostConfig.SecurityOpt = ["no-new-privileges", "label=disable"]; },
    (value) => { value.HostConfig.Privileged = true; },
    (value) => { value.HostConfig.CapAdd = ["SYS_ADMIN"]; },
    (value) => { value.HostConfig.IpcMode = "host"; },
    (value) => { value.HostConfig.IpcMode = "container:other"; },
    (value) => { value.HostConfig.PidMode = "host"; },
    (value) => { value.HostConfig.PidMode = "container:other"; },
    (value) => { value.HostConfig.Devices = [{ PathOnHost: "/dev/null" }]; },
    (value) => { value.HostConfig.DeviceRequests = [{ Driver: "nvidia" }]; },
    (value) => { value.HostConfig.DeviceCgroupRules = ["c 1:3 rwm"]; },
    (value) => { value.Mounts[0].RW = false; },
    (value) => { value.HostConfig.Tmpfs["/tmp"] = "rw,size=1"; },
    (value) => { value.State.ExitCode = 1; },
    (value) => { value.Image = `sha256:${"5".repeat(64)}`; },
    (value) => { value.ImageManifestDescriptor.digest = `sha256:${"5".repeat(64)}`; },
  ]) {
    const value = containerInspection();
    mutate(value);
    assert.throws(() => validateSmokeContainerInspection(value, expectedContainer));
  }
  for (const securityOption of ["no-new-privileges", "no-new-privileges:true",
    "no-new-privileges=true"]) {
    const value = containerInspection();
    value.HostConfig.SecurityOpt = [securityOption];
    assert.doesNotThrow(() => validateSmokeContainerInspection(value, expectedContainer));
  }
});

test("qualification is exact, cleaned, and keeps OCI identities distinct", () => {
  const inspection = report();
  assert.equal(validateImageInspectionReport(inspection, expected()), inspection);
  const value = {
    version: 1, qualification: "isolated_measured_runner_image_only", sourceCommit,
    sourceArchiveSha256: "c".repeat(64), buildEpoch: 1761955200,
    builder: { image:
      "moby/buildkit@sha256:b651bf250c2805569f1db385ad1ea8fb73737dfc851de305f274ca02690fa93f",
    version: "v0.34.1" },
    inspection,
    container: { identityProtocol: "docker_containerd_oci_manifest_v1",
      imageManifestSha256: inspection.oci.manifestSha256,
      imageConfigSha256: inspection.oci.imageConfigSha256,
      networkMode: "none", readOnlyRootfs: true, exitCode: 0,
      securityBoundaryVerified: true },
    toolSourcesSha256: "d".repeat(64),
    cleanup: { containerRemoved: true, builderRemoved: true },
  };
  assert.equal(validateQualification(value, sourceCommit), value);
  for (const expectedSourceCommit of invalidSourceCommits) {
    assert.throws(() => validateQualification(value, expectedSourceCommit));
  }
  for (const mutate of [
    (item) => { item.extra = true; },
    (item) => { item.container.networkMode = "bridge"; },
    (item) => { item.container.imageConfigSha256 = item.inspection.oci.manifestSha256; },
    (item) => { item.container.imageManifestSha256 = item.inspection.oci.imageConfigSha256; },
    (item) => { item.container.identityProtocol = "docker_classic_config_v1"; },
    (item) => { item.cleanup.builderRemoved = false; },
    (item) => { item.builder.version = "v0.33.0"; },
    (item) => { item.inspection.expected.sourceCommit = "e".repeat(40); },
  ]) {
    const item = structuredClone(value);
    mutate(item);
    assert.throws(() => validateQualification(item, sourceCommit));
  }
});

test("source head is an independently supplied current-source boundary", () => {
  assert.equal(validateExpectedSourceHead(`${sourceCommit}\n`, sourceCommit), sourceCommit);
  for (const [head, expectedSourceCommit] of [
    ["e".repeat(40), sourceCommit],
    ...invalidSourceCommits.map((expectedSourceCommit) => [sourceCommit, expectedSourceCommit]),
  ]) assert.throws(() => validateExpectedSourceHead(head, expectedSourceCommit));
});

test("report validation rejects malformed independent expected source commits", () => {
  for (const sourceCommit of invalidSourceCommits) {
    assert.throws(() => validateImageInspectionReport(report(), { ...expected(), sourceCommit }));
  }
});

test("build failure diagnostic forwards only a bounded fixed-log tail", async () => {
  const calls = [];
  const result = await captureBuildFailureDiagnostic(async (args, options) => {
    calls.push({ args, options });
    return "bounded compiler failure\n";
  }, async (tail) => {
    assert.equal(tail, "Combined guest build log tail:\nbounded compiler failure\n");
  });
  assert.equal(result.bytes, Buffer.byteLength("bounded compiler failure\n"));
  assert.match(result.sha256, /^[a-f0-9]{64}$/);
  assert.deepEqual(calls[0], {
    args: ["/usr/bin/tail", "-c", "8192",
      "/var/tmp/bluey-jobs-runner-image-p639/build.log"],
    options: { maximum: 65_536 },
  });
  await assert.rejects(() => captureBuildFailureDiagnostic(async () => "", async () => {}),
    /build_diagnostic_tail/);
  await assert.rejects(() => captureBuildFailureDiagnostic(
    async () => "failure\n", async () => { throw new Error("diagnostic unavailable"); }),
  /diagnostic unavailable/);
  const replacementExpanded = "\ufffd".repeat(8192);
  const replacement = await captureBuildFailureDiagnostic(
    async () => replacementExpanded, async (tail) => assert.equal(tail,
      `Combined guest build log tail:\n${replacementExpanded}`));
  assert.equal(replacement.bytes, 24_576);
  await assert.rejects(() => captureBuildFailureDiagnostic(
    async () => "\ufffd".repeat(11_000), async () => {}), /build_diagnostic_tail/);
});

test("artifact attachment plan binds exactly the inspected archive and complete identity files", () => {
  const inspection = report();
  const source = { sourceCommit, sourceArchiveSha256: "c".repeat(64), toolSourcesSha256: "d".repeat(64) };
  const plan = artifactAttachmentPlan(inspection, "e".repeat(64), { synthetic: "unit-only" }, 1000,
    source, expected());
  assert.deepEqual(Object.keys(plan.sha256s), ["runner.oci.tar", "inventory.json", "measurement.json", "report.json"]);
  assert.equal(plan.sha256s["runner.oci.tar"], "e".repeat(64));
  assert.equal(plan.sha256s["inventory.json"], inspection.oci.inventorySha256);
  assert.equal(plan.sha256s["measurement.json"], inspection.runtime.runtimeMeasurementSha256);
  assert.match(plan.sha256s["report.json"], /^[a-f0-9]{64}$/);
  assert.throws(() => artifactAttachmentPlan(inspection, "0".repeat(64), {}, 1000,
    source, expected()));
  assert.throws(() => artifactAttachmentPlan(inspection, "e".repeat(64), {}, 0,
    source, expected()));
  assert.throws(() => artifactAttachmentPlan(inspection, "e".repeat(64), {}, 1000,
    { ...source, sourceCommit: "f".repeat(40) }, expected()));
  assert.throws(() => artifactAttachmentPlan({ ...inspection,
    expected: { ...inspection.expected, sourceCommit: "f".repeat(40) } }, "e".repeat(64),
  {}, 1000, source, expected()));
  for (const sourceCommit of invalidSourceCommits) {
    assert.throws(() => artifactAttachmentPlan(inspection, "e".repeat(64), {}, 1000,
      source, { ...expected(), sourceCommit }));
  }
});

test("reader qualification precedes builder and validated workload cleanup precedes artifact handoff", async () => {
  const source = await readFile(new URL("./managed-runner-image-workload.mjs", import.meta.url), "utf8");
  const qualify = source.indexOf('guestReaderArguments("guest-probe"');
  const recheck = source.indexOf("await preservation.beforeBuild()");
  const builder = source.indexOf('...dockerEnvironment(), "buildx", "create"');
  const exportIndex = source.lastIndexOf("return completeImageWorkload(provisional");
  const cleanup = source.indexOf("? await cleanupManagedRunnerImageResources(guest, cleanupState)");
  assert(qualify >= 0 && qualify < recheck && recheck < builder);
  assert(exportIndex > source.indexOf('"inspection_image_binding"'));
  assert(exportIndex > cleanup);
  assert(exportIndex > source.lastIndexOf("return preserveFailedArchive(primaryFailure"));
  assert(source.includes('preservation.onPhase("failure_diagnostic")'));
  assert(source.includes('preservation.onPhase("cleanup", { failed:'));
  assert(source.includes('preservation.onPhase("cleaned")'));
  assert(source.includes('...(preservation ? ["managed-runner-artifacts.py"] : [])'));
  assert(!source.includes("apt-get") && !source.includes("apk add"));
});

test("artifact handoff cannot precede verified cleanup and export failure yields no image result", async () => {
  const inspection = report();
  const provisional = { version: 1, qualification: "isolated_measured_runner_image_only", sourceCommit,
    sourceArchiveSha256: "c".repeat(64), buildEpoch: 1761955200,
    builder: { image: "moby/buildkit@sha256:b651bf250c2805569f1db385ad1ea8fb73737dfc851de305f274ca02690fa93f",
      version: "v0.34.1" }, inspection,
    container: { identityProtocol: "docker_containerd_oci_manifest_v1",
      imageManifestSha256: inspection.oci.manifestSha256, imageConfigSha256: inspection.oci.imageConfigSha256,
      networkMode: "none", readOnlyRootfs: true, exitCode: 0, securityBoundaryVerified: true },
    toolSourcesSha256: "d".repeat(64) };
  const plan = { synthetic: "unit-only" };
  const calls = [];
  const preservation = { exportAttachments: async (value) => calls.push(value) };
  for (const cleanup of [{ containerRemoved: false, builderRemoved: true },
    { containerRemoved: true, builderRemoved: false }]) {
    await assert.rejects(() => completeImageWorkload(
      provisional, cleanup, preservation, plan, sourceCommit));
  }
  assert.deepEqual(calls, []);
  const cleanup = { containerRemoved: true, builderRemoved: true };
  const result = await completeImageWorkload(provisional, cleanup, preservation, plan, sourceCommit);
  assert.deepEqual(calls, [plan]);
  assert.deepEqual(result.cleanup, cleanup);
  await assert.rejects(() => completeImageWorkload(provisional, cleanup,
    { exportAttachments: async () => { throw new Error("export failed"); } }, plan,
  sourceCommit), /export failed/);
  assert.deepEqual((await completeImageWorkload(
    provisional, cleanup, null, null, sourceCommit)).cleanup, cleanup);
});

test("cleanup attempts both resources and fails closed without swallowing either failure",
  async () => {
    const calls = [];
    const success = async (args) => { calls.push(args); return ""; };
    assert.deepEqual(await cleanupManagedRunnerImageResources(success,
      { containerAttempted: true, builderAttempted: true }),
    { containerRemoved: true, builderRemoved: true });
    assert(calls.some((args) => args.includes("rm") && args.includes("p639-image-smoke")));
    assert(calls.some((args) => args.includes("buildx") && args.includes("p639-image")));
    let builderRemovalAttempted = false;
    const failure = async (args) => {
      if (args.includes("rm") && args.includes("p639-image-smoke")) throw new Error("container");
      if (args.includes("buildx") && args.includes("rm")) builderRemovalAttempted = true;
      return "";
    };
    await assert.rejects(() => cleanupManagedRunnerImageResources(failure,
      { containerAttempted: true, builderAttempted: true }),
    /managed_runner_image_cleanup_failed/);
    assert.equal(builderRemovalAttempted, true);
  });

function failedDescriptor() {
  return { version: 1, artifact: "runner.oci.tar", size: 17, sha256: "a".repeat(64),
    reader: { synthetic: "unit-only" },
    source: { dev: "1", ino: "2", uid: 1000, mode: 0o400, nlink: 1,
      size: 17, mtimeNs: "3", ctimeNs: "4" } };
}

test("failure descriptor closes archive size digest owner mode identity and reader before inspection", () => {
  const descriptor = failedDescriptor();
  const frozen = validateFailedArchiveDescriptor(descriptor, descriptor.reader, 1000);
  assert(Object.isFrozen(frozen) && Object.isFrozen(frozen.source));
  for (const patch of [{ artifact: "report.json" }, { path: "private-value" }, { size: 0 },
    { size: 4 * 1024 ** 3 + 1 }, { sha256: "0".repeat(64) }, { reader: {} }]) {
    assert.throws(() => validateFailedArchiveDescriptor({ ...descriptor, ...patch }, descriptor.reader, 1000));
  }
  for (const patch of [{ uid: 1001 }, { mode: 0o600 }, { nlink: 2 }, { ino: "0" },
    { ino: "private-value" }, { size: 16 }, { ctimeNs: "-1" }]) {
    assert.throws(() => validateFailedArchiveDescriptor({ ...descriptor,
      source: { ...descriptor.source, ...patch } }, descriptor.reader, 1000));
  }
});

test("failure retention requires cleaned resources and always rethrows the original image failure", async () => {
  const primary = new Error("original_image_failure");
  const cleanup = { builderRemoved: true, containerRemoved: true };
  const plan = { descriptor: failedDescriptor() };
  const calls = [];
  const preservation = { exportFailedArchive: async (value) => { calls.push(value); return false; } };
  for (const bad of [{ builderRemoved: false, containerRemoved: true },
    { builderRemoved: true, containerRemoved: false }]) {
    await assert.rejects(() => preserveFailedArchive(primary, bad, preservation, plan), /failed_archive_cleanup/);
  }
  assert.deepEqual(calls, []);
  await assert.rejects(() => preserveFailedArchive(primary, cleanup, preservation, plan), (error) => error === primary);
  assert.deepEqual(calls, [plan]);
  await assert.rejects(() => preserveFailedArchive(primary, cleanup,
    { exportFailedArchive: async () => { throw new Error("separate_retention_failure"); } }, plan),
  (error) => error === primary);
  await assert.rejects(() => preserveFailedArchive(primary, cleanup, preservation, null), (error) => error === primary);
  assert.deepEqual(calls, [plan]);
});

test("failure archive checkpoint and handoff cannot precede immutable archive or cleanup checks", async () => {
  const source = await readFile(new URL("./managed-runner-image-workload.mjs", import.meta.url), "utf8");
  const freeze = source.indexOf("failedArchivePlan = Object.freeze");
  assert(freeze > source.indexOf('"400",'));
  assert(freeze > source.indexOf('"failed_archive_size_changed"'));
  assert(freeze < source.indexOf('await emitWorkloadProgress(progress, "image_inspection_started")'));
  const handoff = source.lastIndexOf("return preserveFailedArchive(primaryFailure");
  assert(handoff > source.indexOf("if (cleanupFailure)"));
  assert(handoff > source.indexOf("if (freezeFailure) throw freezeFailure"));
  assert(handoff < source.lastIndexOf("return completeImageWorkload"));
});
