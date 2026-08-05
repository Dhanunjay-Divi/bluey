import { openNativeRunnerStorageRoot } from "../runner/dist/native-runner-storage.js";

const configuredRoot = process.env.BLUEY_JOBS_RUNNER_NATIVE_SMOKE_ROOT;
if (!configuredRoot) {
  throw new Error("BLUEY_JOBS_RUNNER_NATIVE_SMOKE_ROOT is required");
}

const root = openNativeRunnerStorageRoot(configuredRoot);
root.assertUnchanged();
const smokeDirectoryName = `native-addon-smoke-${process.pid}`;
const smokeDirectory = await root.ensureDirectory([smokeDirectoryName]);

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

console.log("Bluey Jobs native runner addon smoke passed.");
