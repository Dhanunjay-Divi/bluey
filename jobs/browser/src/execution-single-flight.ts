export interface ExecutionFlightKey {
  runId: string;
  applicationIdentityId: string;
}

interface ActiveExecution<T = unknown> extends ExecutionFlightKey {
  promise: Promise<T>;
}

export class ExecutionSingleFlight {
  private readonly byRun = new Map<string, ActiveExecution>();
  private readonly runByIdentity = new Map<string, string>();

  run<T>(
    key: ExecutionFlightKey,
    operation: () => Promise<T>,
    identityBusyError: () => Error,
  ): Promise<T> {
    const existingRun = this.byRun.get(key.runId);
    if (existingRun) {
      return existingRun.applicationIdentityId === key.applicationIdentityId
        ? existingRun.promise as Promise<T>
        : Promise.reject(identityBusyError());
    }
    if (this.runByIdentity.has(key.applicationIdentityId)) {
      return Promise.reject(identityBusyError());
    }

    let resolveFlight!: (value: T | PromiseLike<T>) => void;
    let rejectFlight!: (reason?: unknown) => void;
    const promise = new Promise<T>((resolve, reject) => {
      resolveFlight = resolve;
      rejectFlight = reject;
    });
    const active: ActiveExecution<T> = { ...key, promise };
    this.byRun.set(key.runId, active);
    this.runByIdentity.set(key.applicationIdentityId, key.runId);

    void Promise.resolve().then(operation).then(
      (value) => {
        this.release(active);
        resolveFlight(value);
      },
      (error) => {
        this.release(active);
        rejectFlight(error);
      },
    );
    return promise;
  }

  private release(active: ActiveExecution): void {
    if (this.byRun.get(active.runId)?.promise === active.promise) {
      this.byRun.delete(active.runId);
    }
    if (this.runByIdentity.get(active.applicationIdentityId) === active.runId) {
      this.runByIdentity.delete(active.applicationIdentityId);
    }
  }
}
