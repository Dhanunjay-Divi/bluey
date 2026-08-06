import { app, net, powerMonitor, shell as electronShell } from "electron";
import { BrowserShell } from "./browser-shell.js";
import type { ControllerPrimaryAction } from "./controller-contract.js";
import {
  continueCurrentLocalRun,
  handleLocalProtocolUrl,
  initializeLocalRunController,
  openCurrentApplicationBrowser,
  refreshLocalControllerForConnectivity,
  requestSafeLocalStop,
  setLocalApplicationsPaused,
  setLocalPowerAvailable,
  shutdownLocalRunController,
} from "./run-controller.js";
import {
  BACKGROUND_LAUNCH_ARGUMENT,
  isBackgroundLoginLaunch,
  LoginItemController,
} from "./login-item-controller.js";
import {
  enqueueProtocolArguments,
  PendingProtocolQueue,
} from "./pending-protocol-queue.js";
import { PowerAdmissionGuard } from "./power-admission.js";
import {
  claimBrowserBuildProof,
  loadPackagedBrowserBuildProof,
} from "./packaged-release.js";

const PROTOCOL_PREFIX = "bluey-jobs://";
const CONNECTIVITY_POLL_MS = 15_000;

export async function startBlueyBrowserApplication(): Promise<void> {
  app.setName("Bluey Browser");
  const electronVersion = process.versions.electron;
  if (!electronVersion) {
    throw new Error("Bluey Browser must run inside Electron");
  }
  const verifiedBuild = await loadPackagedBrowserBuildProof({
    isPackaged: app.isPackaged,
    resourcesPath: process.resourcesPath,
    appVersion: app.getVersion(),
    electronVersion,
    platform: process.platform,
    architecture: process.arch,
    ...developmentReleaseAuthority(),
  });
  if (app.isPackaged) app.setAsDefaultProtocolClient("bluey-jobs");

  const singleInstance = app.requestSingleInstanceLock();
  if (!singleInstance) {
    app.quit();
    return;
  }

  let browserShell: BrowserShell | undefined;
  let quitPrepared = false;
  let quitStarted = false;
  let ready = false;
  let connectivityTimer: NodeJS.Timeout | undefined;
  const pendingProtocolUrls = new PendingProtocolQueue();
  const powerAdmission = new PowerAdmissionGuard();
  const updatePowerAdmission = (): void => {
    if (!ready) return;
    setLocalPowerAvailable(powerAdmission.decision().allowed);
  };
  const onSuspend = (): void => {
    powerAdmission.setSuspended(true);
    updatePowerAdmission();
  };
  const onResume = (): void => {
    powerAdmission.setSuspended(false);
    updatePowerAdmission();
  };
  const onLock = (): void => {
    powerAdmission.setLocked(true);
    updatePowerAdmission();
  };
  const onUnlock = (): void => {
    powerAdmission.setLocked(false);
    updatePowerAdmission();
  };

  const dispatchProtocol = (url: string): void => {
    if (!url.startsWith(PROTOCOL_PREFIX)) return;
    if (!ready) {
      pendingProtocolUrls.push(url);
      return;
    }
    void handleLocalProtocolUrl(url);
  };

  const requestQuit = async (): Promise<void> => {
    if (quitStarted) return;
    quitStarted = true;
    if (connectivityTimer) clearInterval(connectivityTimer);
    powerMonitor.removeListener("suspend", onSuspend);
    powerMonitor.removeListener("resume", onResume);
    powerMonitor.removeListener("lock-screen", onLock);
    powerMonitor.removeListener("unlock-screen", onUnlock);
    browserShell?.beginQuit();
    await shutdownLocalRunController().catch(() => undefined);
    browserShell?.dispose();
    quitPrepared = true;
    app.quit();
  };

  app.on("second-instance", (_event, commandLine) => {
    const protocolUrl = commandLine.find((argument) => argument.startsWith(PROTOCOL_PREFIX));
    if (protocolUrl) dispatchProtocol(protocolUrl);
    browserShell?.show();
  });
  app.on("open-url", (event, url) => {
    event.preventDefault();
    dispatchProtocol(url);
  });
  app.on("before-quit", (event) => {
    if (quitPrepared) return;
    event.preventDefault();
    void requestQuit();
  });
  enqueueProtocolArguments(pendingProtocolUrls, process.argv);

  await app.whenReady();
  const loginItemSupported = app.isPackaged
    && (process.platform === "darwin" || process.platform === "win32");
  const loginItemArguments = process.platform === "win32"
    ? [BACKGROUND_LAUNCH_ARGUMENT]
    : [];
  const loginItemSettings = loginItemSupported
    ? app.getLoginItemSettings(
        loginItemArguments.length > 0 ? { args: loginItemArguments } : undefined,
      )
    : undefined;
  const backgroundLoginLaunch = loginItemSupported && isBackgroundLoginLaunch({
    platform: process.platform,
    argv: process.argv,
    ...(process.platform === "darwin"
      ? { wasOpenedAtLogin: loginItemSettings?.wasOpenedAtLogin }
      : {}),
  });
  powerAdmission.setLocked(powerMonitor.getSystemIdleState(1) === "locked");
  powerMonitor.on("suspend", onSuspend);
  powerMonitor.on("resume", onResume);
  powerMonitor.on("lock-screen", onLock);
  powerMonitor.on("unlock-screen", onUnlock);
  browserShell = new BrowserShell(
    app.getPath("userData"),
    app.getAppPath(),
    {
      onPrimary: (action) => handlePrimaryAction(action),
      onOpenBrowser: () => openCurrentApplicationBrowser(),
      onPauseChange: (next) => {
        setLocalApplicationsPaused(next);
      },
      onStop: async () => {
        await requestSafeLocalStop();
      },
      onOpenJobs: () => openBlueyJobs(),
      onQuit: requestQuit,
    },
    {
      backgroundLoginLaunch,
      loginItems: new LoginItemController(
        { setLoginItemSettings: (settings) => app.setLoginItemSettings(settings) },
        loginItemSupported,
        loginItemArguments,
        loginItemSettings?.openAtLogin,
      ),
    },
  );

  const handlePrimaryAction = async (action: ControllerPrimaryAction): Promise<void> => {
    switch (action) {
      case "open_jobs":
        await openBlueyJobs();
        break;
      case "open_browser":
        await openCurrentApplicationBrowser();
        break;
      case "continue":
        await continueCurrentLocalRun();
        break;
      case "resume":
        setLocalApplicationsPaused(false);
        break;
      case "none":
        break;
    }
  };

  const openBlueyJobs = async (): Promise<void> => {
    await electronShell.openExternal(blueyJobsUrl());
  };

  await browserShell.start(net.isOnline());
  await initializeLocalRunController(
    browserShell,
    claimBrowserBuildProof(verifiedBuild),
  );
  ready = true;
  updatePowerAdmission();
  app.on("activate", () => browserShell?.show());

  let lastOnline = net.isOnline();
  connectivityTimer = setInterval(() => {
    const online = net.isOnline();
    if (online === lastOnline) return;
    lastOnline = online;
    browserShell?.updateConnectivity(online);
    refreshLocalControllerForConnectivity(online);
  }, CONNECTIVITY_POLL_MS);
  connectivityTimer.unref();

  for (const url of pendingProtocolUrls.drain()) dispatchProtocol(url);

}

function developmentReleaseAuthority(): {
  developmentReleaseDirectory?: string;
} {
  if (app.isPackaged) return {};
  const configured = process.env.BLUEY_JOBS_BROWSER_DEVELOPMENT_RELEASE_DIRECTORY;
  if (!configured) return {};
  const apiOrigin = process.env.BLUEY_JOBS_API_ORIGIN;
  if (!apiOrigin) {
    throw new Error("Development Browser release authority requires a loopback Jobs API");
  }
  const url = new URL(apiOrigin);
  if (
    url.protocol !== "http:" ||
    !["127.0.0.1", "localhost"].includes(url.hostname) ||
    url.username ||
    url.password
  ) {
    throw new Error("Development Browser release authority requires a loopback Jobs API");
  }
  return { developmentReleaseDirectory: configured };
}

function blueyJobsUrl(): string {
  const configured = process.env.BLUEY_JOBS_PORTAL_ORIGIN
    || "https://bluey.sh/jobs";
  const url = new URL(configured);
  if (url.username || url.password
    || (url.protocol !== "https:"
      && !(url.protocol === "http:" && ["127.0.0.1", "localhost"].includes(url.hostname)))) {
    return "https://bluey.sh/jobs";
  }
  return url.toString();
}
