# IMPL: Jobs track-before-provider-import guard

This private Phase 665 successor validates an account-scoped Career Track before
an importer can read a candidate-supplied public job URL, and preserves the
post-import validation before storage for in-flight Track changes.

The focused synthetic SQLite/importer test proves early foreign-Track refusal,
post-import removal refusal, and a valid save. It does not submit applications,
contact a provider, enable a feature, or qualify deployment.
