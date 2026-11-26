import assert from "node:assert/strict";
import test from "node:test";
import { createLocalQaLinuxSignupRequestPolicy,
  requireLocalQaSampleResumeReadback } from "./local-qa-linux-signup-gateway.mjs";
import { LOCAL_QA_SAMPLE_RESUME } from "./local-qa-browser-sample-resume.mjs";

const origin = "http://127.0.0.1:48123";
const password = `Qa-${"a".repeat(48)}`;
const token = "a.b.c";
const manifest = Object.freeze({ "/assets/site.js": "b".repeat(64) });

function request({ path, method = "GET", headers = {}, body = null }) {
  return Object.freeze({
    url: () => `${origin}${path}`,
    method: () => method,
    headers: () => headers,
    postData: () => body,
  });
}

function policy(includeSampleResume, includePrepare) {
  return createLocalQaLinuxSignupRequestPolicy({ origin, password, manifest, includeSampleResume, includePrepare });
}

async function startAndConfirm(guarded) {
  const fingerprint = "11111111-1111-4111-8111-111111111111";
  const start = JSON.stringify({ email: "candidate@bluey-qa.test", password, terms_accepted: true,
    turnstile_token: null, device_fingerprint: fingerprint });
  await guarded.authorize(request({ path: "/auth/signup/start", method: "POST",
    headers: { "content-type": "application/json" }, body: start }));
  const otp = guarded.acceptOtp("123456");
  const confirm = JSON.stringify({ email: "candidate@bluey-qa.test", otp, device_fingerprint: fingerprint });
  const forwarded = await guarded.authorize(request({ path: "/auth/signup/confirm", method: "POST",
    headers: { "content-type": "application/json" }, body: confirm }));
  guarded.forwarded(forwarded);
}

test("signup policy admits the exact synthetic signup and post-confirm portal reads", async () => {
  const guarded = policy();
  await guarded.authorize(request({ path: "/jobs/" }));
  await guarded.authorize(request({ path: "/auth/captcha/config" }));
  await startAndConfirm(guarded);
  for (const path of ["/account/me", "/api/jobs/beta-access", "/api/jobs/workspace", "/api/jobs/taxonomy"]) {
    await guarded.authorize(request({ path, headers: { authorization: `Bearer ${token}`, "content-type": "application/json" } }));
  }
  guarded.complete();
});

const sample = LOCAL_QA_SAMPLE_RESUME;
const sampleId = "2bcb52db-e5ff-4778-ad83-872fb7b033f1";
const sampleProfile = Object.freeze({ full_name: sample.name, headline: sample.headline,
  email: sample.candidateEmail, onboarding_complete: false });
const sampleBody = Object.freeze({ file_name: sample.fileName, media_type: sample.mimeType,
  request_id: sampleId, bytes_base64: Buffer.from(sample.text).toString("base64"), profile: sampleProfile });
function upload(body = sampleBody, authorization = `Bearer ${token}`) {
  return request({ path: "/api/jobs/resume-source", method: "POST",
    headers: { "content-type": "application/json", authorization }, body: JSON.stringify(body) });
}

test("only the post-signup import mode can send exactly one reviewed sample upload", async () => {
  const guarded = policy(true);
  assert.throws(() => guarded.beginSampleImport(), /local_qa_linux_signup_gateway_refused/);
  await assert.rejects(guarded.authorize(upload()), /local_qa_linux_signup_gateway_refused/);
  await startAndConfirm(guarded);
  await assert.rejects(guarded.authorize(upload()), /local_qa_linux_signup_gateway_refused/);
  guarded.beginSampleImport();
  assert.throws(() => guarded.complete(), /local_qa_linux_signup_gateway_refused/);
  await guarded.authorize(upload());
  guarded.complete();
  await assert.rejects(guarded.authorize(upload()), /local_qa_linux_signup_gateway_refused/);
  assert.throws(() => guarded.complete(), /local_qa_linux_signup_gateway_refused/);

  const signupOnly = policy(false);
  await startAndConfirm(signupOnly);
  assert.throws(() => signupOnly.beginSampleImport(), /local_qa_linux_signup_gateway_refused/);
  await assert.rejects(signupOnly.authorize(upload()), /local_qa_linux_signup_gateway_refused/);
  signupOnly.complete();
});

test("changed bytes, wrong identity, and missing bearer never pass the sample upload gate", async () => {
  for (const input of [upload({ ...sampleBody, bytes_base64: "eA==" }),
    upload({ ...sampleBody, profile: { ...sampleProfile, email: "else@bluey-qa.test" } }), upload(sampleBody, "")]) {
    const guarded = policy(true);
    await startAndConfirm(guarded);
    guarded.beginSampleImport();
    await assert.rejects(guarded.authorize(input), /local_qa_linux_signup_gateway_refused/);
    assert.throws(() => guarded.complete(), /local_qa_linux_signup_gateway_refused/);
    await assert.rejects(guarded.authorize(upload()), /local_qa_linux_signup_gateway_refused/);
  }
});

test("Prepare admits only the bound match after signup and import, never submission", async () => {
  const guarded = policy(true, true);
  const jobId = "9ec9e42c-1897-4fd1-b885-81cf6abdc198";
  const headers = { "content-type": "application/json", authorization: `Bearer ${token}` };
  const body = { job_id: jobId, mode: "factual", submission_mode: "review_first" };
  const prepare = request({ path: "/api/jobs/applications", method: "POST", headers, body: JSON.stringify(body) });
  const communications = "/api/jobs/communication-actions?limit=100";
  assert.throws(() => policy(false, true), /gateway_refused/);
  assert.throws(() => guarded.beginPrepare(jobId), /gateway_refused/);
  await assert.rejects(guarded.authorize(prepare), /gateway_refused/);
  await startAndConfirm(guarded);
  guarded.beginSampleImport();
  await guarded.authorize(upload());
  guarded.beginPrepare(jobId);
  await guarded.authorize(request({ path: "/jobs/matches" }));
  await assert.rejects(guarded.authorize(request({ path: `${communications}&application_id=${sampleId}`, headers })), /gateway_refused/);
  assert.equal(await guarded.authorize(prepare), "prepare");
  guarded.finishPrepare(sampleId);
  guarded.complete();
  assert.equal(await guarded.authorize(request({ path: communications, headers })), "all_communications");
  assert.equal(await guarded.authorize(request({ path: `${communications}&application_id=${sampleId}`, headers })), "application_communications");
  for (const refused of [prepare, request({ path: `${communications}&application_id=${jobId}`, headers }),
    request({ path: `/api/jobs/applications/${sampleId}/submit`, method: "POST", headers })]) {
    await assert.rejects(guarded.authorize(refused), /gateway_refused/);
  }
  assert.throws(() => guarded.complete(), /gateway_refused/);
  for (const changed of [{ job_id: sampleId }, { mode: "enhance" }, { submission_mode: "auto_submit" }]) {
    const sampleGuard = policy(true, true);
    await startAndConfirm(sampleGuard);
    sampleGuard.beginSampleImport();
    await sampleGuard.authorize(upload());
    sampleGuard.beginPrepare(jobId);
    await assert.rejects(sampleGuard.authorize(request({ path: "/api/jobs/applications", method: "POST", headers,
      body: JSON.stringify({ ...body, ...changed }) })), /gateway_refused/);
    assert.throws(() => sampleGuard.finishPrepare(sampleId), /gateway_refused/);
  }
});

test("independent readback binds both profile and asset to the same browser upload ID and hash", () => {
  const asset = { id: sampleId, file_name: sample.fileName, media_type: sample.mimeType,
    size_bytes: Buffer.byteLength(sample.text), sha256: sample.sha256 };
  const profile = { ...sampleProfile, source_resume_asset_id: sampleId, source_resume_sha256: sample.sha256,
    source_resume_name: sample.fileName, source_resume_media_type: sample.mimeType };
  requireLocalQaSampleResumeReadback({ asset, profile, assetId: sampleId });
  assert.throws(() => requireLocalQaSampleResumeReadback({ asset, assetId: sampleId,
    profile: { ...profile, source_resume_asset_id: "d13858b3-4dfb-466e-bdf4-41c213e8131c" } }),
  /local_qa_linux_signup_gateway_refused/);
  assert.throws(() => requireLocalQaSampleResumeReadback({ profile, assetId: sampleId,
    asset: { ...asset, sha256: "0".repeat(64) } }), /local_qa_linux_signup_gateway_refused/);
});

test("signup policy denies pre-confirm reads and any non-exact credential write", async () => {
  const guarded = policy();
  assert.throws(() => guarded.forwarded("confirm"), /local_qa_linux_signup_gateway_refused/);
  await assert.rejects(guarded.authorize(request({ path: "/api/jobs/workspace",
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" } })), /local_qa_linux_signup_gateway_refused/);
  const badStart = JSON.stringify({ email: "other@bluey-qa.test", password, terms_accepted: true,
    turnstile_token: null, device_fingerprint: "fingerprint" });
  await assert.rejects(guarded.authorize(request({ path: "/auth/signup/start", method: "POST",
    headers: { "content-type": "application/json" }, body: badStart })), /local_qa_linux_signup_gateway_refused/);
  await assert.rejects(guarded.authorize(request({ path: "/api/jobs/onboarding/complete", method: "POST",
    headers: { "content-type": "application/json" }, body: "{}" })), /local_qa_linux_signup_gateway_refused/);
});

test("a wrong OTP or mismatched fingerprint never opens the authenticated reads", async () => {
  for (const [otp, fingerprint] of [["123457", "same"], ["123456", "different"]]) {
    const guarded = policy();
    const headers = { "content-type": "application/json" };
    await guarded.authorize(request({ path: "/auth/signup/start", method: "POST", headers,
      body: JSON.stringify({ email: "candidate@bluey-qa.test", password, terms_accepted: true,
        turnstile_token: null, device_fingerprint: "same" }) }));
    guarded.acceptOtp("123456");
    await assert.rejects(guarded.authorize(request({ path: "/auth/signup/confirm", method: "POST", headers,
      body: JSON.stringify({ email: "candidate@bluey-qa.test", otp, device_fingerprint: fingerprint }) })),
    /local_qa_linux_signup_gateway_refused/);
    assert.throws(() => guarded.forwarded("confirm"), /local_qa_linux_signup_gateway_refused/);
    await assert.rejects(guarded.authorize(request({ path: "/account/me",
      headers: { authorization: `Bearer ${token}` } })), /local_qa_linux_signup_gateway_refused/);
  }
});

test("mock-receipt mode stays opt-in and requires its exact bound resume read", async () => {
  const jobId = "9ec9e42c-1897-4fd1-b885-81cf6abdc198";
  const applicationId = "3f71c23b-b88e-4b0f-8834-31ba68b7f8e6";
  const resumeId = "42c9a369-9a6b-49a4-927c-7be7f1f5b5ec";
  const headers = { "content-type": "application/json", authorization: `Bearer ${token}` };
  const prepare = request({ path: "/api/jobs/applications", method: "POST", headers,
    body: JSON.stringify({ job_id: jobId, mode: "factual", submission_mode: "review_first" }) });
  const resume = request({ path: `/api/jobs/resume-versions/${resumeId}`, headers });

  const inert = policy(true, true);
  await startAndConfirm(inert);
  inert.beginSampleImport();
  await inert.authorize(upload());
  inert.beginPrepare(jobId);
  await inert.authorize(prepare);
  inert.finishPrepare(applicationId);
  await assert.rejects(inert.authorize(resume), /gateway_refused/);
  inert.complete();

  const guarded = createLocalQaLinuxSignupRequestPolicy({
    origin, password, manifest, includeSampleResume: true, includePrepare: true, includeMockReceipt: true,
  });
  await startAndConfirm(guarded);
  guarded.beginSampleImport();
  await guarded.authorize(upload());
  guarded.beginPrepare(jobId);
  await guarded.authorize(prepare);
  assert.throws(() => guarded.beginMockReceiptRead(resumeId), /gateway_refused/);
  guarded.finishPrepare(applicationId, resumeId);
  assert.throws(() => guarded.complete(), /gateway_refused/);
  assert.throws(() => guarded.beginMockReceiptRead("f13858b3-4dfb-466e-bdf4-41c213e8131c"), /gateway_refused/);
  guarded.beginMockReceiptRead(resumeId);
  assert.deepEqual(await guarded.authorize(resume), Object.freeze({ kind: "prepared_resume", token }));
  guarded.complete();
});

test("mock-receipt mode refuses altered or effectful requests after Prepare", async () => {
  const jobId = "9ec9e42c-1897-4fd1-b885-81cf6abdc198";
  const applicationId = "3f71c23b-b88e-4b0f-8834-31ba68b7f8e6";
  const resumeId = "42c9a369-9a6b-49a4-927c-7be7f1f5b5ec";
  const headers = { "content-type": "application/json", authorization: `Bearer ${token}` };
  const prepare = request({ path: "/api/jobs/applications", method: "POST", headers,
    body: JSON.stringify({ job_id: jobId, mode: "factual", submission_mode: "review_first" }) });
  const ready = async () => {
    const guarded = createLocalQaLinuxSignupRequestPolicy({
      origin, password, manifest, includeSampleResume: true, includePrepare: true, includeMockReceipt: true,
    });
    await startAndConfirm(guarded);
    guarded.beginSampleImport();
    await guarded.authorize(upload());
    guarded.beginPrepare(jobId);
    await guarded.authorize(prepare);
    guarded.finishPrepare(applicationId, resumeId);
    guarded.beginMockReceiptRead(resumeId);
    return guarded;
  };
  const exact = `/api/jobs/resume-versions/${resumeId}`;
  for (const input of [
    request({ path: `${exact}?x=1`, headers }),
    request({ path: exact, headers: { ...headers, cookie: "session=forbidden" } }),
    request({ path: exact, headers: { ...headers, authorization: "Bearer changed.token.value" } }),
    request({ path: `/api/jobs/resume-versions/${applicationId}`, headers }),
    request({ path: `/api/jobs/applications/${applicationId}/approve`, method: "POST", headers, body: "{}" }),
    request({ path: `/api/jobs/applications/${applicationId}/submit`, method: "POST", headers, body: "{}" }),
    request({ path: `/api/jobs/internal/applications/${applicationId}/receipt`, method: "POST", headers, body: "{}" }),
  ]) {
    const guarded = await ready();
    await assert.rejects(guarded.authorize(input), /gateway_refused/);
  }

  const duplicate = await ready();
  assert.deepEqual(
    await duplicate.authorize(request({ path: exact, headers })),
    Object.freeze({ kind: "prepared_resume", token }),
  );
  await assert.rejects(duplicate.authorize(request({ path: exact, headers })), /gateway_refused/);
});
