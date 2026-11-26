// One observed PostgreSQL control field, not a complete resource/ownership proof.
// Keep uint64 identity bytes intact rather than rounding them through Number.
export function parsePostgresSystemIdentifier(value) {
  if (
    typeof value !== "string" ||
    !/^[1-9][0-9]{0,19}$/.test(value) ||
    BigInt(value) > 18446744073709551615n
  ) {
    throw new Error("Non-production PostgreSQL system identifier is invalid");
  }
  return value;
}
