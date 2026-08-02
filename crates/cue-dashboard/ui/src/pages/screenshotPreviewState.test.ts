import { describe, expect, it } from "vitest";
import {
  INITIAL_SCREENSHOT_FLOW_STATE,
  MAX_SCREENSHOT_TITLE_CHARS,
  captureKindLabel,
  classifyScreenshotError,
  formatScreenshotBytes,
  isWindowsUserAgent,
  normalizeScreenshotTitle,
  screenshotFlowReducer,
  screenshotReceiptNotice,
  unicodeCharCount,
  validateScreenshotTitle,
  type ContextArtifactSummary,
  type ScreenshotContextAttachReceipt,
  type ScreenshotPreview,
} from "./screenshotPreviewState";

const preview: ScreenshotPreview = {
  operation_id: "63ba3207-8be6-4fd0-ae79-3ea9f5162e11",
  path: "/private/tmp/bluey-preview.png",
  file_size_bytes: 1_572_864,
  created_at: "1720000000000",
  capture_kind: "region_or_window",
};

const artifact: ContextArtifactSummary = {
  id: preview.operation_id,
  kind: "image",
  title: "Architecture diagram",
  processing_status: "pending",
};

const activeReceipt: ScreenshotContextAttachReceipt = {
  operation_id: preview.operation_id,
  session_id: "a352d90c-684c-4100-bc11-ea06480a6bad",
  artifact,
  already_attached: false,
  active: true,
};

describe("screenshot preview flow", () => {
  it("requires an explicit capture before a preview exists", () => {
    const capturing = screenshotFlowReducer(INITIAL_SCREENSHOT_FLOW_STATE, {
      type: "capture_started",
      request: "selection",
    });
    expect(capturing).toMatchObject({ phase: "capturing", preview: null, capture_request: "selection" });

    const ready = screenshotFlowReducer(capturing, { type: "capture_succeeded", preview });
    expect(ready).toMatchObject({
      phase: "preview",
      preview,
      title: "Selected screen context",
      capture_request: null,
    });
  });

  it("consumes the temporary preview only after attachment succeeds", () => {
    const ready = screenshotFlowReducer(
      screenshotFlowReducer(INITIAL_SCREENSHOT_FLOW_STATE, {
        type: "capture_started",
        request: "selection",
      }),
      { type: "capture_succeeded", preview },
    );
    const attaching = screenshotFlowReducer(ready, { type: "attach_started" });
    expect(attaching).toMatchObject({ phase: "attaching", preview });

    const attached = screenshotFlowReducer(attaching, { type: "attach_succeeded", receipt: activeReceipt });
    expect(attached).toMatchObject({
      phase: "attached",
      preview: null,
      attached_receipt: activeReceipt,
      notice: "Screenshot attached to the current session.",
    });
  });

  it("keeps the preview available when attachment or discard fails", () => {
    const ready = screenshotFlowReducer(
      screenshotFlowReducer(INITIAL_SCREENSHOT_FLOW_STATE, {
        type: "capture_started",
        request: "selection",
      }),
      { type: "capture_succeeded", preview },
    );

    const attachError = classifyScreenshotError("attach", "active session unavailable");
    const afterAttachFailure = screenshotFlowReducer(
      screenshotFlowReducer(ready, { type: "attach_started" }),
      { type: "operation_failed", error: attachError },
    );
    expect(afterAttachFailure).toMatchObject({ phase: "preview", preview, error: attachError });

    const discardError = classifyScreenshotError("discard", "file is busy");
    const afterDiscardFailure = screenshotFlowReducer(
      screenshotFlowReducer(afterAttachFailure, { type: "discard_started" }),
      { type: "operation_failed", error: discardError },
    );
    expect(afterDiscardFailure).toMatchObject({ phase: "preview", preview, error: discardError });
  });

  it("clears the preview after explicit discard without claiming attachment", () => {
    const ready = screenshotFlowReducer(
      screenshotFlowReducer(INITIAL_SCREENSHOT_FLOW_STATE, {
        type: "capture_started",
        request: "full_screen",
      }),
      { type: "capture_succeeded", preview: { ...preview, capture_kind: "full_screen" } },
    );
    const discarded = screenshotFlowReducer(
      screenshotFlowReducer(ready, { type: "discard_started" }),
      { type: "discard_succeeded" },
    );
    expect(discarded.phase).toBe("idle");
    expect(discarded.preview).toBeNull();
    expect(discarded.notice).toMatch(/not attached/);
  });

  it("ignores impossible success events instead of inventing state", () => {
    expect(
      screenshotFlowReducer(INITIAL_SCREENSHOT_FLOW_STATE, {
        type: "attach_succeeded",
        receipt: activeReceipt,
      }),
    ).toBe(INITIAL_SCREENSHOT_FLOW_STATE);
  });

  it("distinguishes a current attachment from an idempotent receipt in a non-active session", () => {
    expect(screenshotReceiptNotice(activeReceipt)).toBe("Screenshot attached to the current session.");
    expect(screenshotReceiptNotice({ ...activeReceipt, already_attached: true })).toBe(
      "Screenshot attachment confirmed in the current session.",
    );
    expect(screenshotReceiptNotice({ ...activeReceipt, active: false, already_attached: true })).toBe(
      "Screenshot attachment confirmed in a saved, non-active session.",
    );
    expect(screenshotReceiptNotice({ ...activeReceipt, active: false })).toBe(
      "Screenshot saved to a non-active session.",
    );
  });
});

describe("screenshot title boundary", () => {
  it("collapses whitespace and counts Unicode characters", () => {
    expect(normalizeScreenshotTitle("  System\n  design\t diagram  ")).toBe("System design diagram");
    expect(unicodeCharCount("A😀B")).toBe(3);
  });

  it("requires a title and mirrors the 160-character server limit", () => {
    expect(validateScreenshotTitle(" \n ")).toMatch(/Enter a title/);
    expect(validateScreenshotTitle("t".repeat(MAX_SCREENSHOT_TITLE_CHARS))).toBeNull();
    expect(validateScreenshotTitle("t".repeat(MAX_SCREENSHOT_TITLE_CHARS + 1))).toMatch(/160/);
  });
});

describe("screenshot diagnostics", () => {
  it("distinguishes cancellation, permission denial, and other failures", () => {
    expect(classifyScreenshotError("capture", "User cancelled selection")).toMatchObject({
      kind: "cancelled",
      title: "Capture cancelled",
    });
    expect(classifyScreenshotError("capture", "Screen Recording permission denied")).toMatchObject({
      kind: "permission",
    });
    expect(classifyScreenshotError("capture", "Capture was cancelled or permission was denied")).toMatchObject({
      kind: "ambiguous",
      title: "Capture did not complete",
    });
    expect(classifyScreenshotError("attach", "No active session")).toMatchObject({
      kind: "other",
      operation: "attach",
    });
    expect(
      classifyScreenshotError(
        "attach",
        "Bluey could not confirm whether the screenshot was attached. Its private copies were preserved; retry Attach to reconcile safely.",
      ),
    ).toMatchObject({
      kind: "ambiguous",
      operation: "attach",
      title: "Attachment needs reconciliation",
    });
    expect(
      classifyScreenshotError(
        "attach",
        "Bluey could not confirm whether the screenshot was attached.",
      ).message,
    ).toMatch(/discarding it cannot undo/i);
  });

  it("labels capture kinds and formats byte counts", () => {
    expect(captureKindLabel("full_screen")).toBe("Full screen");
    expect(captureKindLabel("window")).toBe("Window");
    expect(captureKindLabel("region_or_window")).toBe("Selected region or window");
    expect(formatScreenshotBytes(512)).toBe("512 B");
    expect(formatScreenshotBytes(1_572_864)).toBe("1.5 MB");
    expect(isWindowsUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe(true);
    expect(isWindowsUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X)")).toBe(false);
  });
});
