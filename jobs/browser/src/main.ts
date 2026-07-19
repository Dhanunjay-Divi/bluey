import { startBlueyBrowserApplication } from "./app-lifecycle.js";
import { app } from "electron";

void startBlueyBrowserApplication().catch((error: unknown) => {
  console.error("Bluey Browser startup failed", {
    code: "browser_startup_failed",
    name: error instanceof Error ? error.name : "unknown",
  });
  process.exitCode = 1;
  app.quit();
});
