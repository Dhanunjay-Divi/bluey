import { NativeConnection, Worker } from "@temporalio/worker";
import * as activities from "./activities.js";

async function run(): Promise<void> {
  const address = process.env.TEMPORAL_ADDRESS;
  const namespace = process.env.TEMPORAL_NAMESPACE;
  if (!address || !namespace) throw new Error("TEMPORAL_ADDRESS and TEMPORAL_NAMESPACE are required");
  const connection = await NativeConnection.connect({ address });
  const worker = await Worker.create({
    connection,
    namespace,
    taskQueue: process.env.BLUEY_JOBS_TASK_QUEUE || "bluey-jobs-applications",
    workflowsPath: new URL("./workflows.js", import.meta.url).pathname,
    activities,
  });
  await worker.run();
}

void run();
