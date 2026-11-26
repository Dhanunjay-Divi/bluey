/** Called inside the profile mutation lock, before starting or resuming execution. */
export async function recoverOrExecuteRun<T>(operations: {
  readCommitted(): Promise<T | undefined>;
  requireMutation(): Promise<void>;
  assertNoDurableAmbiguity(): Promise<void>;
  recoverSubmittedCheckpoint(): Promise<T | undefined>;
  execute(): Promise<T>;
}): Promise<T> {
  const existing = await operations.readCommitted();
  if (existing !== undefined) return existing;

  await operations.requireMutation();
  const recoveredDuringAdmission = await operations.readCommitted();
  if (recoveredDuringAdmission !== undefined) return recoveredDuringAdmission;

  await operations.assertNoDurableAmbiguity();
  const recovered = await operations.recoverSubmittedCheckpoint();
  if (recovered !== undefined) return recovered;

  return operations.execute();
}
