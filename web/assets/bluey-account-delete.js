(function bootstrapBlueyAccountDeletion(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) {
    module.exports = api;
  }
  if (root && typeof root === 'object') {
    root.BlueyAccountDeletion = api;
  }
})(typeof globalThis === 'object' ? globalThis : null, function createBlueyAccountDeletion() {
  'use strict';

  const DEFAULT_PENDING_MESSAGE =
    'Account deletion is securely pending runner-volume and storage cleanup. '
    + 'Your sign-in was kept so Bluey can check again.';

  function deletionDecision(result) {
    const status = result?.status;
    const body = result?.body;
    const verifiedDeleted = status === 200
      && body?.deleted === true
      && body?.state === 'deleted'
      && typeof body?.deleted_at === 'string'
      && body.deleted_at.length > 0;
    if (verifiedDeleted) {
      return Object.freeze({ kind: 'deleted', body });
    }
    if (status === 202 && body?.deleted === false) {
      const note = typeof body.note === 'string' && body.note.trim()
        ? body.note
        : DEFAULT_PENDING_MESSAGE;
      return Object.freeze({ kind: 'pending', body, note });
    }
    throw new Error(
      `Account deletion returned an inconsistent completion response (HTTP ${String(status)}); `
      + 'your sign-in was kept.',
    );
  }

  async function run({ request, onPending, onDeleted }) {
    if (typeof request !== 'function'
      || typeof onPending !== 'function'
      || typeof onDeleted !== 'function') {
      throw new TypeError('Invalid account deletion handler configuration.');
    }
    const decision = deletionDecision(await request());
    if (decision.kind === 'pending') {
      await onPending(decision.note, decision.body);
    } else {
      await onDeleted(decision.body);
    }
    return decision;
  }

  return Object.freeze({ deletionDecision, run });
});
