import assert from "node:assert/strict";
import test from "node:test";
import { parsePostgresSystemIdentifier } from "./nonprod-postgres-identity.mjs";

test("preserves canonical positive uint64 boundary identities as strings", () => {
  for (const value of ["1", "9223372036854775808", "18446744073709551615"]) {
    assert.equal(parsePostgresSystemIdentifier(value), value);
    assert.equal(JSON.stringify({ systemIdentifier: parsePostgresSystemIdentifier(value) }),
      `{"systemIdentifier":"${value}"}`);
  }
});

test("rejects zero and values outside the uint64 identity range", () => {
  for (const value of ["0", "18446744073709551616", "99999999999999999999"]) {
    assert.throws(() => parsePostgresSystemIdentifier(value), /system identifier is invalid/);
  }
});

test("rejects noncanonical decimal spellings instead of normalizing identities", () => {
  for (const value of ["", "01", "-1", "+1", "1.0", "1e3", "0x10", " 1", "1 ", "1\n"]) {
    assert.throws(() => parsePostgresSystemIdentifier(value), /system identifier is invalid/);
  }
});

test("rejects non-string inputs without coercion or numeric precision loss", () => {
  for (const value of [1, 1n, null, undefined, {}, [], Number.MAX_SAFE_INTEGER]) {
    assert.throws(() => parsePostgresSystemIdentifier(value), /system identifier is invalid/);
  }
});

test("bounds oversized identifiers before bigint parsing", () => {
  assert.throws(() => parsePostgresSystemIdentifier("9".repeat(1024)), /system identifier is invalid/);
});

test("rejects non-ASCII digits and malformed text", () => {
  for (const value of ["١", "１", "1\u0000", "1\ud800", "NaN", "Infinity"]) {
    assert.throws(() => parsePostgresSystemIdentifier(value), /system identifier is invalid/);
  }
});

test("does not echo a rejected value into diagnostics", () => {
  assert.throws(() => parsePostgresSystemIdentifier("private-rejected-input"), (error) => {
    assert.equal(error.message, "Non-production PostgreSQL system identifier is invalid");
    assert(!error.message.includes("private-rejected-input"));
    return true;
  });
});
