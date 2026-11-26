import assert from "node:assert/strict";
import test from "node:test";
import {
  createLocalQaLinuxPostmasterObserver,
  parseLinuxPostmasterStat,
  requireOwnedLinuxLoopbackListener,
  requireLinuxPostmasterUid,
  requireOwnedLinuxPgListener,
} from "./local-qa-linux-postmaster-observer.mjs";

const pid = 12345;
const port = 43107;
const inode = "9385650";
const header = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode";
const table = (...lines) => [header, ...lines, ""].join("\n");
const table6 = (...lines) => [header.replace("rem_address", "remote_address"), ...lines, ""].join("\n");
const v4 = "0100007F", v4any = "00000000", v6 = "00000000000000000000000001000000";
const serverRow = ({ address = v4, number = port, state = "0A", socket = inode, slot = 0 } = {}) =>
  ` ${slot}: ${address}:${number.toString(16).toUpperCase().padStart(4, "0")} ${"0".repeat(address.length)}:0000 ${state} ` +
  `00000000:00000000 00:00000000 00000000 999 0 ${socket} 1 0000000000000000 100 0 0 10 0`;
const stat = ({ quoted = "postgres (owned)", parent = "888", group = "888", start = "9007199254740993" } = {}) => {
  const fields = Array(50).fill("0");
  fields.splice(0, 4, "S", parent, group, group);
  fields[19] = start;
  return `${pid} (${quoted}) ${fields.join(" ")}\n`;
};

test("Linux proc stat binds the requested PID and birth stamp with punctuation in comm", () => {
  assert.deepEqual(parseLinuxPostmasterStat(stat(), pid), {
    pid, ppid: 888, pgrp: 888, startTime: "9007199254740993",
  });
  for (const text of [stat({ start: "01" }), stat({ start: "18446744073709551616" }), stat() + "\n",
    stat({ group: "-1" }), stat().replace("12345", "12346"), `${pid} (postgres) S 1\n`]) {
    assert.throws(() => parseLinuxPostmasterStat(text, pid));
  }
});

test("process ownership requires one real/effective/saved/filesystem UID row", () => {
  const row = "Name:\tpostgres\nUid:\t999\t999\t999\t999\nThreads:\t1\n";
  assert.equal(requireLinuxPostmasterUid(row, 999), true);
  for (const text of [row.replace("999\t999", "999\t1000"), row + "Uid:\t999\t999\t999\t999\n",
    row.replace("Uid:", "Gid:"), row.replace("999\t999", "0999\t999")]) {
    assert.throws(() => requireLinuxPostmasterUid(text, 999));
  }
});

test("only an owned IPv4 loopback LISTEN inode proves the expected PostgreSQL endpoint", () => {
  const tcp4 = table(serverRow(), serverRow({ number: 55321, state: "01", socket: "9385651", slot: 1 }),
    serverRow({ number: 55322, socket: "9990909", slot: 2 }));
  assert.equal(requireOwnedLinuxPgListener(tcp4, table6(), new Set([inode, "9385651"]), port), inode);
  assert.throws(() => requireOwnedLinuxPgListener(tcp4, table6(), new Set(["9385651"]), port));
});

test("API readiness may wait for an empty port but must refuse occupied and unowned listeners", () => {
  assert.equal(requireOwnedLinuxLoopbackListener(table(), table6(), new Set(), port, true), null);
  assert.throws(() => requireOwnedLinuxPgListener(table(), table6(), new Set(), port));
  assert.throws(() => requireOwnedLinuxLoopbackListener(table(serverRow()), table6(), new Set(), port, true));
  assert.throws(() => requireOwnedLinuxLoopbackListener(table(serverRow({ number: 55321 })), table6(), new Set([inode]), port, true));
});

test("rejects wildcard, IPv6, additional owned, competing and malformed listener rows", () => {
  for (const [v4table, v6table, owners] of [
    [table(serverRow({ address: v4any })), table6(), new Set([inode])],
    [table(serverRow()), table6(serverRow({ address: v6, socket: "9234567" })), new Set([inode, "9234567"])],
    [table(serverRow(), serverRow({ number: 55321, socket: "9234567", slot: 1 })), table6(), new Set([inode, "9234567"])],
    [table(serverRow(), serverRow({ socket: "9234567", slot: 1 })), table6(), new Set([inode])],
    [table(serverRow(), serverRow({ address: v4any, socket: "9234567", slot: 1 })), table6(), new Set([inode])],
    [table(serverRow({ number: 55321 })), table6(), new Set([inode])],
    [table(serverRow(), "corrupt row"), table6(), new Set([inode])],
    [table(serverRow().replace("00000000:0000", `${"0".repeat(32)}:0000`)), table6(), new Set([inode])],
  ]) assert.throws(() => requireOwnedLinuxPgListener(v4table, v6table, owners, port));
});

test("the live observer refuses an unbound root before invoking the outer ownership guard", async () => {
  let calls = 0;
  await assert.rejects(createLocalQaLinuxPostmasterObserver({
    root: "/tmp/bluey-test-runs-a1b2c3d4e5f6/run.1795250653.12345.32615190360",
    postgresPath: "/does/not/exist", postgresSha256: "a".repeat(64),
    unitCgroup: "/system.slice/bluey-jobs-test-owned.service", networkNamespace: "net:[4026532555]",
    async assertOwnedUnit() { calls++; return true; },
  }));
  assert.equal(calls, 0);
});
