import assert from "node:assert/strict";
import test from "node:test";

import { completeLocalQaBrowserSignup } from "./local-qa-browser-signup.mjs";

const ORIGIN = "http://127.0.0.1:5191";
const PASSWORD = `Qa-${"a".repeat(48)}`;
const SIGNUP_URL = `${ORIGIN}/login?mode=signup&next=%2Fjobs`;
const JOBS_URL = `${ORIGIN}/jobs/`;

class FakeResponse {
  constructor(path, status, body = {}) {
    this.path = path;
    this.code = status;
    this.body = body;
  }

  url() {
    return `${ORIGIN}${this.path}`;
  }

  request() {
    return { method: () => "POST" };
  }

  status() {
    return this.code;
  }

  json() {
    return this.body;
  }
}

class FakeLocator {
  constructor(page, key) {
    this.page = page;
    this.key = key;
  }

  async waitFor() {
    this.page.actions.push(`visible:${this.key}`);
  }

  async getAttribute(name) {
    this.page.actions.push(`attribute:${this.key}:${name}`);
    return this.key === "link:Start my job search" && name === "href"
      ? "/login?mode=signup&next=%2Fjobs"
      : null;
  }

  async fill() {
    this.page.actions.push(`fill:${this.key}`);
  }

  async check() {
    this.page.actions.push(`check:${this.key}`);
  }

  async click() {
    this.page.actions.push(`click:${this.key}`);
    if (this.key === "link:Start my job search") this.page.navigate(SIGNUP_URL);
    if (this.key === "#confirmSignupButton") this.page.navigate(JOBS_URL);
  }
}

class FakePage {
  constructor({
    startStatus = 200,
    confirmStatus = 200,
    account = {
      id: "01234567-89ab-4cde-8fab-0123456789ab",
      email: "candidate@bluey-qa.test",
      is_admin: false,
      is_temporary: false,
    },
  } = {}) {
    this.current = "about:blank";
    this.actions = [];
    this.urlWaiters = [];
    this.startStatus = startStatus;
    this.confirmStatus = confirmStatus;
    this.account = account;
  }

  url() {
    return this.current;
  }

  async goto(url) {
    this.actions.push(`goto:${url}`);
    this.navigate(url);
  }

  getByRole(role, options) {
    const key = role === "link" ? `link:${options.name}` : `${role}:${options.name}`;
    this.actions.push(`role:${key}`);
    return new FakeLocator(this, key);
  }

  locator(selector) {
    this.actions.push(`locator:${selector}`);
    return new FakeLocator(this, selector);
  }

  waitForURL(url) {
    this.actions.push(`wait-url:${url}`);
    return new Promise((resolve) => this.urlWaiters.push({ url, resolve }));
  }

  waitForResponse(predicate) {
    const response = new FakeResponse(
      this.actions.includes("click:#accountPrimaryButton") ? "/auth/signup/confirm" : "/auth/signup/start",
      this.actions.includes("click:#accountPrimaryButton") ? this.confirmStatus : this.startStatus,
      {
        account: this.account,
      },
    );
    assert.equal(predicate(response), true, "fake response must satisfy exact POST predicate");
    this.actions.push(`wait-response:${response.path}`);
    return Promise.resolve(response);
  }

  navigate(url) {
    this.current = url;
    const matching = this.urlWaiters.filter((waiter) => waiter.url === url);
    this.urlWaiters = this.urlWaiters.filter((waiter) => waiter.url !== url);
    for (const waiter of matching) waiter.resolve();
  }
}

test("fake Page double completes the local candidate signup in response-safe order", async () => {
  const page = new FakePage();
  let takes = 0;
  const result = await completeLocalQaBrowserSignup({
    page,
    origin: ORIGIN,
    password: PASSWORD,
    takeSignupCode: async () => {
      takes += 1;
      return "123456";
    },
  });

  assert.deepEqual(result, { accountId: "01234567-89ab-4cde-8fab-0123456789ab" });
  assert.equal(takes, 1);
  assert.ok(page.actions.indexOf("wait-response:/auth/signup/start") < page.actions.indexOf("click:#accountPrimaryButton"));
  assert.ok(page.actions.indexOf("click:#accountPrimaryButton") < page.actions.indexOf("fill:#signupOtp"));
  assert.ok(page.actions.indexOf("wait-response:/auth/signup/confirm") < page.actions.indexOf("click:#confirmSignupButton"));
  assert.ok(page.actions.indexOf(`wait-url:${JOBS_URL}`) < page.actions.indexOf("click:#confirmSignupButton"));
  assert.ok(page.actions.includes("visible:heading:Import once, then tailor every job"));
});

test("fake Page double refuses a non-fresh page before navigation", async () => {
  const page = new FakePage();
  page.current = `${ORIGIN}/jobs/`;
  await assert.rejects(
    completeLocalQaBrowserSignup({
      page,
      origin: ORIGIN,
      password: PASSWORD,
      takeSignupCode: async () => "123456",
    }),
    /local_qa_browser_signup_refused:fresh_page/,
  );
  assert.deepEqual(page.actions, []);
});

test("fake Page double refuses a failed start without taking an OTP", async () => {
  const page = new FakePage({ startStatus: 400 });
  let takes = 0;
  await assert.rejects(
    completeLocalQaBrowserSignup({
      page,
      origin: ORIGIN,
      password: PASSWORD,
      takeSignupCode: async () => {
        takes += 1;
        return "123456";
      },
    }),
    /local_qa_browser_signup_refused:start_status/,
  );
  assert.equal(takes, 0);
  assert.equal(page.actions.includes("fill:#signupOtp"), false);
});

test("fake Page double refuses a malformed authenticated account subject", async () => {
  const page = new FakePage({
    account: {
      id: "01234567-89ab-4cde-8fab-0123456789ab",
      email: "other@bluey-qa.test",
      is_admin: false,
      is_temporary: false,
    },
  });
  await assert.rejects(
    completeLocalQaBrowserSignup({
      page,
      origin: ORIGIN,
      password: PASSWORD,
      takeSignupCode: async () => "123456",
    }),
    /local_qa_browser_signup_refused:confirm_account/,
  );
});

test("fake Page double refuses non-loopback origin and malformed synthetic passwords", async () => {
  const page = new FakePage();
  await assert.rejects(
    completeLocalQaBrowserSignup({
      page,
      origin: "http://localhost:5187",
      password: PASSWORD,
      takeSignupCode: async () => "123456",
    }),
    /local_qa_browser_signup_refused:origin/,
  );
  await assert.rejects(
    completeLocalQaBrowserSignup({
      page: new FakePage(),
      origin: ORIGIN,
      password: "not-a-synthetic-password",
      takeSignupCode: async () => "123456",
    }),
    /local_qa_browser_signup_refused:input/,
  );
});
