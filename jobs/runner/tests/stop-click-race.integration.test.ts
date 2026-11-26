// Intercepted component regression: synthetic control/storage, not a server or native-custody proof.
import { describe, expect, it } from "vitest";
import { executeStopClickRace } from "./stop-click-race.fixture.js";

describe("browser executor stop versus click race", () => {
  it("refuses the paused pre-click stop before Phase B or the employer POST", async () => {
    const result = await executeStopClickRace("stop-during-preclick-checkpoint");
    const detail = JSON.stringify(result);
    expect(result.posts, detail).toBe(0);
    expect(result.irreversibleCalls, detail).toBe(0);
    expect(result.stopChecks, detail).toBe(2);
    expect(result.finalSubmitAttempted, detail).toBe(false);
    expect(result.finalSubmitAuthorized, detail).toBe(false);
    expect(result.finalSubmitActivationOutcome, detail).toBeUndefined();
    expect(result.checkpointFailures, detail).toBe(0);
    expect(result.checkpointCount, detail).toBe(1);
    expect(result.checkpointMatchesScenario, detail).toBe(true);
    expect(result.checkpointPhase, detail).toBe("final_submit_started");
    expect(result.checkpointStatus, detail).toBe("side_effect_unknown");
    expect(result.checkpointPersistedBeforeStop, detail).toBe(true);
    expect(result.stopDeliveredAtPausedCheckpoint, detail).toBe(true);
    expect(result.errorName, detail).toBe("Error");
    expect(result.errorMessage, detail).toBe("synthetic tracked work stopping");
  }, 30_000);

  it("keeps an aborted response fail-closed after the sole employer POST", async () => {
    const result = await executeStopClickRace("stop-after-employer-post");
    const detail = JSON.stringify(result);
    expect(result.posts, detail).toBe(1);
    expect(result.irreversibleCalls, detail).toBe(1);
    expect(result.checkpointFailures, detail).toBe(0);
    expect(result.checkpointCount, detail).toBe(1);
    expect(result.checkpointMatchesScenario, detail).toBe(true);
    expect(result.checkpointPhase, detail).toBe("side_effect_unknown");
    expect(result.checkpointStatus, detail).toBe("side_effect_unknown");
    expect(result.finalSubmitAttempted, detail).toBe(true);
    expect(result.finalSubmitAuthorized, detail).toBe(true);
    expect(result.finalSubmitActivationOutcome, detail).toBe("activation_uncertain");
    // The fence maps a broad failure path to this class. One intercepted POST
    // is observed; this assertion does not infer a second employer request.
    expect(result.errorName, detail).toBe("AdditionalSubmitRequestError");
    expect(result.errorMessage, detail).toBe("Application submit activation emitted more than one mutating request");
    expect(result.receiptStatus, detail).toBeUndefined();
    expect(result.interventionKind, detail).toBeUndefined();
  }, 30_000);
});
