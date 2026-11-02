import assert from "node:assert/strict";
import { test } from "node:test";
import { validateNativePackagingLock, validateWebPackagingGraph } from "./check-provenance-licenses.mjs";

function fixture() {
  const manifest = { name: "@bluey/jobs-browser-packaging", version: "0.1.0", private: true,
    devDependencies: { "electron-builder": "26.15.3" } };
  const lock = { name: manifest.name, version: manifest.version, lockfileVersion: 3,
    packages: { "": { name: manifest.name, version: manifest.version,
      devDependencies: { ...manifest.devDependencies } }, "node_modules/electron-builder": {
      version: "26.15.3", resolved: "https://registry.npmjs.org/electron-builder/-/electron-builder-26.15.3.tgz",
      integrity: "sha512-" + Buffer.alloc(64, 1).toString("base64"), license: "MIT" } } };
  return { manifest, lock };
}

test("separate native SDK inventory accepts exact private pinned metadata without claiming audit", () => {
  const { manifest, lock } = fixture();
  assert.deepEqual(validateNativePackagingLock(manifest, lock), []);
});

test("separate native SDK rejects mixed manifest identity dependencies or parent workspace policy", () => {
  for (const mutate of [
    ({ manifest }) => { manifest.private = false; },
    ({ manifest }) => { manifest.workspaces = []; },
    ({ manifest }) => { manifest.overrides = { minimatch: "11.0.3" }; },
    ({ manifest, lock }) => { manifest.dependencies = {}; lock.packages[""].dependencies = {}; },
    ({ manifest, lock }) => {
      manifest.devDependencies.extra = "1.0.0";
      lock.packages[""].devDependencies.extra = "1.0.0";
    },
    ({ lock }) => { lock.name = "bluey-jobs"; },
    ({ lock }) => { lock.packages[""].name = "bluey-jobs"; },
    ({ lock }) => { lock.packages[""].version = "0.2.0"; },
    ({ lock }) => { delete lock.packages[""]; },
    ({ lock }) => { lock.packages[""].devDependencies["electron-builder"] = "26.18.0"; },
    ({ manifest, lock }) => {
      manifest.devDependencies["electron-builder"] = "^26.15.3";
      lock.packages[""].devDependencies["electron-builder"] = "^26.15.3";
    },
    ({ lock }) => { lock.packages["node_modules/local"] = { link: true, resolved: "../.." }; },
  ]) {
    const input = fixture(); mutate(input);
    assert.notDeepEqual(validateNativePackagingLock(input.manifest, input.lock), []);
  }
});

test("separate native SDK retains registry integrity and license requirements", () => {
  for (const mutate of [
    (entry) => { entry.resolved = "file:../parent"; },
    (entry) => { entry.resolved = "https://registry.npmjs.org.evil.invalid/package.tgz"; },
    (entry) => { entry.resolved = "https://user:secret@registry.npmjs.org/package.tgz"; },
    (entry) => { entry.resolved = "https://registry.npmjs.org/package.tgz?token=secret"; },
    (entry) => { entry.integrity = "sha512-invalid"; },
    (entry) => { delete entry.integrity; },
    (entry) => { delete entry.license; },
    (entry) => { entry.license = "UNLICENSED"; },
  ]) {
    const { manifest, lock } = fixture();
    mutate(lock.packages["node_modules/electron-builder"]);
    assert.notDeepEqual(validateNativePackagingLock(manifest, lock), []);
  }
});

test("Web/cloud inventory independently enforces modern graph and quarantined SDK separation", () => {
  const manifest = { devDependencies: { "@electron/asar": "4.3.1" },
    overrides: { "@electron/asar@4.3.1": { glob: "14.0.3", minimatch: "11.0.3" } } };
  const lock = { packages: { "": { devDependencies: { ...manifest.devDependencies } },
    browser: { devDependencies: { "@electron/asar": "4.3.1" } },
    "node_modules/@electron/asar": { version: "4.3.1" },
    "node_modules/glob": { version: "14.0.3" }, "node_modules/minimatch": { version: "11.0.3" } } };
  assert.deepEqual(validateWebPackagingGraph(manifest, lock), []);
  for (const [name, version] of [["glob", "13.0.6"], ["minimatch", "10.2.5"], ["@electron/asar", "3.4.1"]]) {
    const changed = structuredClone(lock);
    changed.packages[`node_modules/${name}`].version = version;
    assert.notDeepEqual(validateWebPackagingGraph(manifest, changed), []);
  }
  for (const name of ["electron-builder", "app-builder-lib", "electron-winstaller", "electron-builder-squirrel-windows"]) {
    const changed = structuredClone(lock); changed.packages[`node_modules/${name}`] = { version: "26.15.3" };
    assert.notDeepEqual(validateWebPackagingGraph(manifest, changed), []);
  }
  const missingPolicy = structuredClone(manifest); delete missingPolicy.overrides;
  assert.notDeepEqual(validateWebPackagingGraph(missingPolicy, lock), []);
  const missingDirectDependency = structuredClone(manifest); delete missingDirectDependency.devDependencies;
  assert.notDeepEqual(validateWebPackagingGraph(missingDirectDependency, lock), []);
});
