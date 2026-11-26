import { openNativeRunnerStorageRoot } from "../runner/dist/native-runner-storage.js";

const configuredRoot = process.env.BLUEY_JOBS_RUNNER_NATIVE_SMOKE_ROOT;
if (!configuredRoot) {
  throw new Error("BLUEY_JOBS_RUNNER_NATIVE_SMOKE_ROOT is required");
}

const root = openNativeRunnerStorageRoot(configuredRoot);
root.assertUnchanged();

function requireVolumeIdentityEmptiness(expected, phase) {
  if (root.isEmptyForVolumeIdentity() !== expected) {
    throw new Error(`native addon reported unexpected volume-identity emptiness ${phase}`);
  }
}

requireVolumeIdentityEmptiness(true, "before creating the smoke directory");
const smokeDirectoryName = `native-addon-smoke-${process.pid}`;
const smokeDirectory = await root.ensureDirectory([smokeDirectoryName]);
requireVolumeIdentityEmptiness(false, "with the smoke directory present");

if (!(await smokeDirectory.writeFileExclusive("probe", Buffer.from("bluey", "utf8")))) {
  throw new Error("native addon did not create the smoke file");
}
const contents = await smokeDirectory.readFileBounded("probe", 16);
if (contents.toString("utf8") !== "bluey") {
  throw new Error("native addon returned different smoke bytes");
}
const inventory = await smokeDirectory.inventory();
if (inventory.count !== 1 || inventory.bytes !== 5) {
  throw new Error("native addon returned different smoke inventory");
}
await smokeDirectory.removeEntry("probe");
if ((await smokeDirectory.inventory()).count !== 0) {
  throw new Error("native addon did not remove the smoke file");
}
const rootDirectory = await root.openDirectory([]);
await rootDirectory.removeEntry(smokeDirectoryName);
root.assertUnchanged();
requireVolumeIdentityEmptiness(true, "after removing the smoke directory");

console.log("Bluey Jobs native runner addon smoke passed.");
