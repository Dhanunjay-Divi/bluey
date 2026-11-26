import assert from "node:assert/strict";
import { managedReceiptCases } from "./managed-receipt-postgres-proof.mjs";

export const MANAGED_RECEIPT_CARGO_COMMAND = Object.freeze([
  "cargo", "test", "--manifest-path", "server/Cargo.toml", "--lib", "--locked", "--offline",
  "--features", "integration-test-support", "--jobs", "2",
]);

// Pure source transform for a fresh, separately admitted capsule. The caller
// must bind the base source and emitted bytes. This module never runs a child.
export function renderManagedReceiptPostgresWrapper(source, profile = "baseline") {
  const names = managedReceiptCases(profile).map(({ name }) => name);
  function replaceOnce(before, after) {
    assert.equal(source.split(before).length, 2, "unique receipt wrapper preimage");
    source = source.replace(before, after);
  }

  const importLine = 'import { parsePostgresSystemIdentifier } from "./nonprod-postgres-identity.mjs";';
  replaceOnce(importLine, `${importLine}
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
const receiptCases = Object.freeze(${JSON.stringify(names)});`);
  const separator = 'assert.equal(args.shift(), "--", "command separator required (omit command for cluster smoke)");';
  replaceOnce(separator, `${separator}
assert.deepEqual(caseFilters, receiptCases, "exact managed receipt selectors required");
assert.deepEqual(args, ${JSON.stringify(MANAGED_RECEIPT_CARGO_COMMAND)},
  "exact managed receipt library command required");
assert.equal(realpathSync(process.cwd()), resolve(fileURLToPath(new URL("../..", import.meta.url))),
  "managed receipt wrapper must run from its staged source root");`);
  if (profile === "interview-prep-browser") {
    const work = 'const work = join(root, "nonprod-postgres");';
    replaceOnce(work, `assert(process.platform === "linux" && process.arch === "x64",
  "owned receipt browser requires Linux x64");
assert.equal(process.env.BLUEY_INTERVIEW_PREP_OUTER_FENCE, "qualified-linux-interview-prep-fixture-v1",
  "owned receipt browser requires explicit outer controller admission");
env.BLUEY_INTERVIEW_PREP_OUTER_FENCE = process.env.BLUEY_INTERVIEW_PREP_OUTER_FENCE;
${work}`);
  }
  const begin = "      for (const filter of caseFilters) {";
  const end = "      let passed = 0;";
  const from = source.indexOf(begin);
  const to = source.indexOf(end);
  assert.equal(source.split(begin).length, 2, "unique filtered inventory preimage");
  assert.equal(source.split(end).length, 2, "unique case execution preimage");
  assert(from >= 0 && to > from, "wrapper inventory order");
  replaceOnce(source.slice(from, to), [
    '      const listed = await runChild([...args, "--", "--list"], childEnv, true);',
    '      assert(listed.code === 0 && listed.signal === null && !listed.error, "Cargo inventory failed");',
    '      const inventoryLines = listed.stdout.trim().split("\\n");',
    '      const inventorySummary = /^([1-9][0-9]*) tests?, 0 benchmarks?$/.exec(inventoryLines.pop());',
    '      const inventory = inventoryLines.filter(Boolean).map((line) => {',
    '        const row = /^([A-Za-z_][A-Za-z0-9_:]{0,1023}): test$/.exec(line);',
    '        assert(row, "invalid Cargo inventory row");',
    '        return row[1];',
    '      });',
    '      assert(inventorySummary && String(inventory.length) === inventorySummary[1]',
    '        && inventory.length <= 20_000 && new Set(inventory).size === inventory.length, "invalid Cargo inventory");',
    '      for (const name of receiptCases) {',
    '        assert(inventory.includes(name), "exact receipt case absent from full inventory");',
    '        names.add(name);',
    '      }',
    '      console.log(JSON.stringify({ event: "nonprod_postgres_test_inventory", names: inventory,',
    '        count: inventory.length, sha256: createHash("sha256").update(JSON.stringify(inventory)).digest("hex") }));',
    '',
  ].join("\n"));
  return source;
}
