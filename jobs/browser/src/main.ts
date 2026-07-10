import { app, BrowserWindow, ipcMain, safeStorage } from "electron";
import { chromium, type BrowserContext } from "playwright";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { submissionPolicy } from "@bluey/jobs-automation";

interface LinkAccountRequest {
  accountId: string;
  accessToken: string;
}

interface StartRunRequest {
  accountId: string;
  runId: string;
  applicationId: string;
  url: string;
}

let window: BrowserWindow | null = null;
const contexts = new Map<string, BrowserContext>();
const singleInstance = app.requestSingleInstanceLock();

app.setName("Bluey Browser");
app.setAsDefaultProtocolClient("bluey-jobs");

if (!singleInstance) {
  app.quit();
} else {
  app.on("second-instance", (_event, commandLine) => {
    const protocolUrl = commandLine.find((argument) => argument.startsWith("bluey-jobs://"));
    if (protocolUrl) openProtocolUrl(protocolUrl);
    showWindow();
  });
  app.on("open-url", (event, url) => {
    event.preventDefault();
    openProtocolUrl(url);
  });
}

app.whenReady().then(() => {
  window = new BrowserWindow({
    width: 520,
    height: 420,
    minWidth: 440,
    minHeight: 340,
    show: false,
    backgroundColor: "#070a0c",
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  void window.loadURL(statusPage());
  window.once("ready-to-show", () => window?.show());
  app.on("activate", showWindow);
});

ipcMain.handle("jobs:link-account", async (_event, request: LinkAccountRequest) => {
  assertIdentifier(request.accountId, "accountId");
  if (!request.accessToken) throw new Error("Bluey sign-in token is required");
  if (!safeStorage.isEncryptionAvailable()) throw new Error("Secure system storage is unavailable");
  const directory = accountDirectory(request.accountId);
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, "account.auth"), safeStorage.encryptString(request.accessToken), {
    mode: 0o600,
  });
  return { linked: true };
});

ipcMain.handle("jobs:start-run", async (_event, request: StartRunRequest) => {
  assertIdentifier(request.accountId, "accountId");
  assertIdentifier(request.runId, "runId");
  const decision = submissionPolicy(request.url);
  if (decision.policy !== "automate") {
    return { status: decision.policy, reason: decision.reason };
  }
  const token = await readAccessToken(request.accountId);
  if (!token) throw new Error("Sign in to Bluey Browser first");
  const context = await contextFor(request.accountId);
  const page = await context.newPage();
  await page.goto(request.url, { waitUntil: "domcontentloaded", timeout: 45_000 });
  await page.bringToFront();
  return {
    status: "running",
    runId: request.runId,
    applicationId: request.applicationId,
    title: await page.title(),
    url: page.url(),
  };
});

async function contextFor(accountId: string): Promise<BrowserContext> {
  const existing = contexts.get(accountId);
  if (existing) return existing;
  const profile = join(accountDirectory(accountId), "chromium-profile");
  await mkdir(profile, { recursive: true });
  const context = await chromium.launchPersistentContext(profile, {
    headless: false,
    channel: "chromium",
    viewport: null,
    acceptDownloads: true,
  });
  contexts.set(accountId, context);
  context.on("close", () => contexts.delete(accountId));
  return context;
}

async function readAccessToken(accountId: string): Promise<string> {
  if (!safeStorage.isEncryptionAvailable()) return "";
  try {
    const encrypted = await readFile(join(accountDirectory(accountId), "account.auth"));
    return safeStorage.decryptString(encrypted);
  } catch {
    return "";
  }
}

function accountDirectory(accountId: string): string {
  const key = createHash("sha256").update(accountId).digest("hex").slice(0, 24);
  return join(app.getPath("userData"), "profiles", key);
}

function assertIdentifier(value: string, label: string): void {
  if (!/^[A-Za-z0-9_-]{3,160}$/.test(value)) throw new Error(`Invalid ${label}`);
}

function openProtocolUrl(rawUrl: string): void {
  try {
    const url = new URL(rawUrl);
    if (url.protocol !== "bluey-jobs:" || !["open", "takeover"].includes(url.hostname)) return;
    showWindow();
  } catch {
    // Ignore malformed external protocol requests.
  }
}

function showWindow(): void {
  if (!window) return;
  if (window.isMinimized()) window.restore();
  window.show();
  window.focus();
}

function statusPage(): string {
  const html = `<!doctype html><html><meta charset="utf-8"><style>
  :root{color-scheme:dark;font-family:Inter,-apple-system,sans-serif;background:#070a0c;color:#f3f7f9}
  body{margin:0;padding:32px}main{border:1px solid #26323a;border-radius:8px;padding:24px;background:#0b0e10}
  b{color:#58d3ff}p{color:#9eabb3;line-height:1.5}small{color:#68747c}
  </style><body><main><b>bluey jobs</b><h1>Bluey Browser</h1><p>Your separate application profile is ready. Start a reviewed application from Bluey Jobs to open it here.</p><small>Job-site sign-ins stay separate from your everyday browser.</small></main></body></html>`;
  return `data:text/html;charset=utf-8,${encodeURIComponent(html)}`;
}

app.on("before-quit", () => {
  for (const context of contexts.values()) void context.close();
});
