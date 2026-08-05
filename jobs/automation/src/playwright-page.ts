import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import { readFile, stat } from "node:fs/promises";
import type {
  BrowserContext,
  Locator,
  Page,
  Request,
  Route,
  WebSocketRoute,
} from "playwright";
import type {
  BrowserLocator,
  BrowserPage,
  CertifiedFinalSubmitAdapter,
  EffectiveSubmitTargetIdentity,
  ExactSubmitExpectation,
  ExactSubmitFieldEvidence,
  ExactSubmitFileEvidence,
  ExactSubmitFormEvidence,
  ExactSubmitPartOrderEntry,
  FormControl,
  FormControlKind,
  FormFileEvidence,
  TrustedSubmitFieldValue,
} from "./contracts.js";
import {
  FORM_FILE_READBACK_LIMITS,
  snapshotFileIdentity,
} from "./form-readback.js";
import { certifiedProviderJobKey } from "./provider-job-key.js";
import {
  inspectCertifiedSubmitForm,
  type CertifiedSubmitFormPart,
  type CertifiedSubmitFormSnapshot,
} from "./certified-submit-form.js";
import {
  AdditionalSubmitRequestError,
  assertExactOutgoingSubmit,
  createOwnedExactSubmitRequest,
  ExactSubmitEvidenceError,
  hydrateExactOutgoingSubmit,
  isSuccessfulExactSubmitHttpStatus,
  type OwnedExactSubmitPart,
  type OwnedExactSubmitRequest,
} from "./trusted-submit.js";
import { EXACT_SUBMIT_FIELD_LIMITS } from "./trusted-submit.js";

const exactSubmitContextGuards = new WeakMap<BrowserContext, PlaywrightExactSubmitContextGuard>();
const exactSubmitPageGuards = new WeakMap<Page, PlaywrightExactSubmitGuard>();

export class PlaywrightBrowserPage implements BrowserPage {
  private readonly exactSubmitGuard: PlaywrightExactSubmitGuard;

  constructor(readonly page: Page) {
    let guard = exactSubmitPageGuards.get(page);
    if (!guard) {
      guard = new PlaywrightExactSubmitGuard(page);
      exactSubmitPageGuards.set(page, guard);
    }
    this.exactSubmitGuard = guard;
  }

  async installExactSubmitGuard(
    adapter: CertifiedFinalSubmitAdapter,
    approvedCanonicalUrl: string,
  ): Promise<void> {
    await this.exactSubmitGuard.install(adapter, approvedCanonicalUrl);
  }

  async beginExactSubmitGuard(): Promise<void> {
    await this.exactSubmitGuard.beginFillBoundary();
  }

  async assertExactSubmitGuardClean(): Promise<void> {
    await this.exactSubmitGuard.assertClean();
  }

  url(): string {
    return this.page.url();
  }

  title(): Promise<string> {
    return this.page.title();
  }

  locator(selector: string): BrowserLocator {
    return new PlaywrightBrowserLocator(this.page.locator(selector), this.exactSubmitGuard);
  }

  async controls(): Promise<FormControl[]> {
    await this.exactSubmitGuard.assertClean();
    const result = await this.page.evaluate(async (limits) => {
      const controls = Array.from(document.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>(
        "input, textarea, select",
      ));
      const selectedFiles = controls.flatMap((control) => (
        control instanceof HTMLInputElement && control.type === "file"
          ? Array.from(control.files || [])
          : []
      ));
      assertBoundedFiles(selectedFiles, limits);
      const fileEvidence = new Map<HTMLInputElement, FormFileEvidence[]>();
      for (const control of controls) {
        if (!(control instanceof HTMLInputElement) || control.type !== "file") continue;
        fileEvidence.set(control, await hashFiles(Array.from(control.files || [])));
      }
      return controls
        .filter((control) => !control.disabled)
        .map((control, index) => {
          const fieldId = control.dataset.blueyFieldId || `field-${index}`;
          control.dataset.blueyFieldId = fieldId;
          const byFor = control.id ? document.querySelector<HTMLLabelElement>(`label[for="${CSS.escape(control.id)}"]`) : null;
          const wrapping = control.closest("label");
          const ariaLabelledBy = control.getAttribute("aria-labelledby")
            ?.split(/\s+/)
            .map((id) => document.getElementById(id)?.textContent || "")
            .join(" ");
          const nearbyLegend = control.closest("fieldset")?.querySelector("legend")?.textContent || "";
          const label = (byFor?.textContent || wrapping?.textContent || control.getAttribute("aria-label")
            || ariaLabelledBy || nearbyLegend || "").replace(/\s+/g, " ").trim();
          const rawType = control instanceof HTMLSelectElement
            ? "select"
            : control instanceof HTMLTextAreaElement
              ? "textarea"
              : control.type.toLowerCase();
          const supported = ["text", "email", "tel", "url", "select", "textarea", "checkbox", "radio", "file", "hidden"];
          const kind = supported.includes(rawType) ? rawType : "other";
          return {
            selector: `[data-bluey-field-id="${fieldId}"]`,
            kind,
            label,
            name: control.getAttribute("name") || control.id || "",
            placeholder: control.getAttribute("placeholder") || "",
            required: control.required || control.getAttribute("aria-required") === "true",
            value: control instanceof HTMLInputElement && control.type === "file"
              ? Array.from(control.files || []).map((file) => file.name).join(", ")
              : control.value,
            checked: control instanceof HTMLInputElement && ["checkbox", "radio"].includes(control.type)
              ? control.checked
              : undefined,
            options: control instanceof HTMLSelectElement
              ? Array.from(control.options).map((option) => ({ label: option.text, value: option.value }))
              : undefined,
            files: control instanceof HTMLInputElement && control.type === "file"
              ? fileEvidence.get(control) ?? []
              : undefined,
          };
        });

      function assertBoundedFiles(
        files: File[],
        bounds: typeof FORM_FILE_READBACK_LIMITS,
      ): void {
        if (files.length > bounds.maxFileCount || !globalThis.crypto?.subtle) {
          throw new Error("Browser could not verify the selected application documents");
        }
        let aggregateBytes = 0;
        for (const file of files) {
          if (!file.name
            || file.name.length > bounds.maxFileNameChars
            || !Number.isSafeInteger(file.size)
            || file.size <= 0
            || file.size > bounds.maxFileBytes
            || typeof file.arrayBuffer !== "function") {
            throw new Error("Browser could not verify the selected application documents");
          }
          aggregateBytes += file.size;
          if (!Number.isSafeInteger(aggregateBytes) || aggregateBytes > bounds.maxAggregateBytes) {
            throw new Error("Browser could not verify the selected application documents");
          }
        }
      }

      async function hashFiles(files: File[]): Promise<FormFileEvidence[]> {
        const evidence: FormFileEvidence[] = [];
        for (const file of files) {
          const bytes = await file.arrayBuffer();
          if (bytes.byteLength !== file.size) {
            throw new Error("Browser could not verify the selected application documents");
          }
          const digest = new Uint8Array(await globalThis.crypto.subtle.digest("SHA-256", bytes));
          evidence.push({
            name: file.name,
            byteLength: bytes.byteLength,
            sha256: Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join(""),
          });
        }
        return evidence;
      }
    }, FORM_FILE_READBACK_LIMITS) as FormControl[];
    await this.exactSubmitGuard.assertClean();
    return result;
  }

  async bodyText(): Promise<string> {
    await this.exactSubmitGuard.assertClean();
    const result = await this.page.locator("body").innerText().catch(() => "");
    await this.exactSubmitGuard.assertClean();
    return result;
  }

  async waitForSettled(): Promise<void> {
    await this.page.waitForLoadState("domcontentloaded", { timeout: 20_000 }).catch(() => undefined);
    await this.page.waitForLoadState("networkidle", { timeout: 3_000 }).catch(() => undefined);
    await this.exactSubmitGuard.assertClean();
  }

  async screenshot(options?: { fullPage?: boolean }): Promise<Uint8Array> {
    await this.exactSubmitGuard.assertClean();
    const screenshot = await this.page.screenshot({ fullPage: options?.fullPage ?? true });
    await this.exactSubmitGuard.assertClean();
    return screenshot;
  }
}

class PlaywrightBrowserLocator implements BrowserLocator {
  constructor(
    private readonly locator: Locator,
    private readonly exactSubmitGuard: PlaywrightExactSubmitGuard,
  ) {}

  count(): Promise<number> {
    return this.guarded(() => this.locator.count());
  }

  async fill(value: string): Promise<void> {
    await this.exactSubmitGuard.assertClean();
    await this.locator.fill(value);
    if (await this.locator.inputValue() !== value) {
      throw new Error("Browser did not retain the requested field value.");
    }
    await this.exactSubmitGuard.assertClean();
  }

  async click(): Promise<void> {
    await this.exactSubmitGuard.assertClean();
    await this.locator.click();
    await this.exactSubmitGuard.assertClean();
  }

  textContent(): Promise<string | null> {
    return this.guarded(() => this.locator.textContent());
  }

  getAttribute(name: string): Promise<string | null> {
    return this.guarded(() => this.locator.getAttribute(name));
  }

  isVisible(): Promise<boolean> {
    return this.guarded(() => this.locator.isVisible());
  }

  async selectOption(value: string): Promise<void> {
    await this.exactSubmitGuard.assertClean();
    const selected = await this.locator.selectOption(value);
    if (selected.length !== 1 || selected[0] !== value) {
      throw new Error("Browser did not retain the requested select option.");
    }
    await this.exactSubmitGuard.assertClean();
  }

  async setChecked(checked: boolean): Promise<void> {
    await this.exactSubmitGuard.assertClean();
    await this.locator.setChecked(checked);
    if (await this.locator.isChecked() !== checked) {
      throw new Error("Browser did not retain the requested checked state.");
    }
    await this.exactSubmitGuard.assertClean();
  }

  async setInputFiles(paths: string[]): Promise<FormFileEvidence[]> {
    await this.exactSubmitGuard.assertClean();
    if (paths.length < 1 || paths.length > FORM_FILE_READBACK_LIMITS.maxFileCount) {
      throw new Error("Browser could not verify the requested application documents.");
    }
    const expected = await this.exactSubmitGuard.rememberSelectedDocuments(paths);
    await this.locator.setInputFiles(paths);
    await this.exactSubmitGuard.assertClean();
    return expected;
  }

  async effectiveSubmitTarget(
    adapter: CertifiedFinalSubmitAdapter,
  ): Promise<EffectiveSubmitTargetIdentity> {
    await this.exactSubmitGuard.assertClean();
    const identity = (await this.exactSubmitGuard.inspectSubmitForm(adapter)).target;
    await this.exactSubmitGuard.assertClean();
    return identity;
  }

  async successfulSubmitEvidence(
    trustedFields: ReadonlyArray<Readonly<TrustedSubmitFieldValue>>,
    providerJobKey: string,
  ): Promise<Readonly<ExactSubmitFormEvidence>> {
    await this.exactSubmitGuard.assertClean();
    const adapter = certifiedAdapterFromProviderJobKey(providerJobKey);
    const snapshot = await this.exactSubmitGuard.inspectSubmitForm(adapter);
    if (snapshot.target.providerJobKey !== providerJobKey) {
      throw new Error("Browser could not identify the application submit fields");
    }
    const trustedByName = new Map<string, string[]>();
    for (const field of trustedFields) {
      const values = trustedByName.get(field.fieldName) ?? [];
      values.push(normalizeMultipartTextValue(field.value));
      trustedByName.set(field.fieldName, values);
    }
    const approvedProviderJobId = providerJobKey.split(":").at(-1);
    if (!approvedProviderJobId) {
      throw new Error("Browser could not identify the application submit fields");
    }
    const trustedOffsets = new Map<string, number>();
    const hiddenNames = new Set<string>();
    const partOrder: ExactSubmitPartOrderEntry[] = [];
    let fieldIndex = 0;
    let fileIndex = 0;
    let aggregateBytes = 0;
    const fields = snapshot.parts.flatMap((part) => {
      if (part.kind === "file") {
        this.exactSubmitGuard.assertSelectedDocument(part);
        partOrder.push(Object.freeze({ kind: "file", index: fileIndex }));
        fileIndex += 1;
        return [];
      }
      const { fieldName } = part;
      if (part.source === "submitter") {
        throw new Error("Browser could not identify the application submit fields");
      }
      if (part.source === "hidden") {
        if (trustedByName.has(fieldName) || hiddenNames.has(fieldName)) {
          throw new Error("Browser could not identify the application submit fields");
        }
        hiddenNames.add(fieldName);
        assertCertifiedHiddenField(adapter, fieldName, part.value, approvedProviderJobId);
      }
      const offset = trustedOffsets.get(fieldName) ?? 0;
      const trustedValues = trustedByName.get(fieldName);
      const value = normalizeMultipartTextValue(part.value);
      if (part.source === "visible") {
        if (!trustedValues || trustedValues[offset] !== value) {
          throw new Error("Browser could not identify the application submit fields");
        }
        trustedOffsets.set(fieldName, offset + 1);
      }
      const valueBytes = Buffer.from(value, "utf8");
      aggregateBytes += valueBytes.byteLength;
      if (valueBytes.byteLength > EXACT_SUBMIT_FIELD_LIMITS.maxValueBytes
        || !Number.isSafeInteger(aggregateBytes)
        || aggregateBytes > EXACT_SUBMIT_FIELD_LIMITS.maxAggregateValueBytes) {
        throw new Error("Browser could not identify the application submit fields");
      }
      partOrder.push(Object.freeze({ kind: "field", index: fieldIndex }));
      fieldIndex += 1;
      return [Object.freeze({
        fieldName,
        valueByteLength: valueBytes.byteLength,
        valueSha256: createHash("sha256").update(valueBytes).digest("hex"),
      })];
    });
    if (Array.from(trustedByName).some(([fieldName, values]) => (
      (trustedOffsets.get(fieldName) ?? 0) !== values.length
    ))) {
      throw new Error("Browser could not identify the application submit fields");
    }
    if (fields.length < 1 || fields.length > EXACT_SUBMIT_FIELD_LIMITS.maxFieldCount) {
      throw new Error("Browser could not identify the application submit fields");
    }
    await this.exactSubmitGuard.assertClean();
    return Object.freeze({
      fields: Object.freeze(fields),
      partOrder: Object.freeze(partOrder),
    });
  }

  async clickWithExactSubmit(expectation: ExactSubmitExpectation): Promise<number> {
    await this.exactSubmitGuard.arm(expectation);
    await Promise.all([
      this.locator.click(),
      this.exactSubmitGuard.waitForExactRequest(),
    ]);
    await this.exactSubmitGuard.assertClean();
    return this.exactSubmitGuard.waitForExactResponseStatus();
  }

  private async guarded<T>(operation: () => Promise<T>): Promise<T> {
    await this.exactSubmitGuard.assertClean();
    const value = await operation();
    await this.exactSubmitGuard.assertClean();
    return value;
  }
}

type ExactSubmitGuardPhase = "new" | "installed" | "begun" | "armed";

class PlaywrightExactSubmitGuard {
  private phase: ExactSubmitGuardPhase = "new";
  private adapter?: CertifiedFinalSubmitAdapter;
  private approvedJobKey?: string;
  private expectation?: ExactSubmitExpectation;
  private ownedRequest?: OwnedExactSubmitRequest;
  private exactRequest?: Request;
  private violation?: Error;
  private closed = false;
  private resolveExactRequest?: () => void;
  private rejectExactRequest?: (error: Error) => void;
  private exactRequestObserved?: Promise<void>;
  private readonly selectedDocuments = new Map<string, SelectedSubmitDocument>();

  constructor(readonly page: Page) {}

  async rememberSelectedDocuments(paths: readonly string[]): Promise<FormFileEvidence[]> {
    const documents: SelectedSubmitDocument[] = [];
    let aggregateBytes = 0;
    for (const path of paths) {
      const identity = snapshotFileIdentity(path);
      const metadata = await stat(path);
      if (!metadata.isFile()
        || !Number.isSafeInteger(metadata.size)
        || metadata.size < 1
        || metadata.size > FORM_FILE_READBACK_LIMITS.maxFileBytes) {
        throw new Error("Browser could not verify the requested application documents.");
      }
      aggregateBytes += metadata.size;
      if (aggregateBytes > FORM_FILE_READBACK_LIMITS.maxAggregateBytes) {
        throw new Error("Browser could not verify the requested application documents.");
      }
      const bytes = await readFile(path);
      const sha256 = createHash("sha256").update(bytes).digest("hex");
      if (bytes.byteLength !== metadata.size || sha256 !== identity.sha256) {
        throw new Error("Browser could not verify the requested application documents.");
      }
      const document = Object.freeze({
        ...identity,
        byteLength: bytes.byteLength,
        bytes: Buffer.from(bytes),
      });
      this.selectedDocuments.set(selectedDocumentKey(document.name, document.sha256), document);
      documents.push(document);
    }
    return documents.map(({ name, sha256, byteLength }) => ({ name, sha256, byteLength }));
  }

  async inspectSubmitForm(
    adapter: CertifiedFinalSubmitAdapter,
  ): Promise<CertifiedSubmitFormSnapshot> {
    await this.assertClean();
    if (this.phase === "new" || adapter !== this.adapter) throw new ExactSubmitEvidenceError();
    const snapshot = await inspectCertifiedSubmitForm(this.page, adapter);
    if (snapshot.target.providerJobKey !== this.approvedJobKey) {
      throw new ExactSubmitEvidenceError();
    }
    await this.assertClean();
    return snapshot;
  }

  assertSelectedDocument(part: Extract<CertifiedSubmitFormPart, { kind: "file" }>): void {
    const document = this.selectedDocuments.get(selectedDocumentKey(part.name, part.sha256));
    if (!document || document.byteLength !== part.byteLength) {
      throw new Error("Browser did not retain the requested application document.");
    }
  }

  async install(
    adapter: CertifiedFinalSubmitAdapter,
    approvedCanonicalUrl: string,
  ): Promise<void> {
    let approvedJobKey: string;
    try {
      approvedJobKey = certifiedProviderJobKey(adapter, approvedCanonicalUrl, "submit");
    } catch {
      throw new ExactSubmitEvidenceError();
    }
    if (this.phase !== "new") {
      if (this.adapter !== adapter
        || this.approvedJobKey !== approvedJobKey) {
        throw new ExactSubmitEvidenceError();
      }
      await this.assertNoServiceWorker();
      await this.assertClean();
      return;
    }

    this.adapter = adapter;
    this.approvedJobKey = approvedJobKey;
    const context = this.page.context();
    let contextGuard = exactSubmitContextGuards.get(context);
    if (!contextGuard) {
      contextGuard = new PlaywrightExactSubmitContextGuard(context);
      exactSubmitContextGuards.set(context, contextGuard);
    }
    await contextGuard.activate(this);
    this.phase = "installed";
    this.page.once("close", () => {
      this.closed = true;
      contextGuard?.deactivate(this);
    });
    await this.assertNoServiceWorker();
    await this.assertClean();
  }

  async beginFillBoundary(): Promise<void> {
    await this.assertClean();
    if (this.phase === "new" || !this.adapter || !this.approvedJobKey) {
      throw new ExactSubmitEvidenceError();
    }
    if (this.phase === "armed") throw new ExactSubmitEvidenceError();
    let currentJobKey: string;
    try {
      currentJobKey = certifiedProviderJobKey(this.adapter, this.page.url(), "submit");
    } catch {
      throw new ExactSubmitEvidenceError();
    }
    if (currentJobKey !== this.approvedJobKey) throw new ExactSubmitEvidenceError();
    this.phase = "begun";
    await this.assertNoServiceWorker();
    await this.assertClean();
  }

  async arm(expectation: ExactSubmitExpectation): Promise<void> {
    await this.assertClean();
    if (this.phase !== "begun"
      || expectation.target.providerJobKey !== this.approvedJobKey
      || this.expectation
      || this.exactRequestObserved) {
      throw new ExactSubmitEvidenceError();
    }
    if (!this.adapter) throw new ExactSubmitEvidenceError();
    const snapshot = await this.inspectSubmitForm(this.adapter);
    const ownedParts = this.validateArmedSnapshot(expectation, snapshot);
    this.ownedRequest = ownedParts
      ? createOwnedExactSubmitRequest(expectation, ownedParts)
      : undefined;
    this.expectation = Object.freeze({
      target: Object.freeze({ ...expectation.target }),
      files: Object.freeze(expectation.files.map((file) => Object.freeze({ ...file }))),
      fields: Object.freeze(expectation.fields.map((field) => Object.freeze({ ...field }))),
      partOrder: Object.freeze(expectation.partOrder.map((entry) => Object.freeze({ ...entry }))),
    });
    this.exactRequestObserved = new Promise<void>((resolve, reject) => {
      this.resolveExactRequest = resolve;
      this.rejectExactRequest = reject;
    });
    this.phase = "armed";
    await this.assertNoServiceWorker();
    await this.assertClean();
  }

  async waitForExactRequest(): Promise<void> {
    if (!this.exactRequestObserved) throw new ExactSubmitEvidenceError();
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      await Promise.race([
        this.exactRequestObserved,
        new Promise<void>((_resolve, reject) => {
          timeout = setTimeout(() => reject(new ExactSubmitEvidenceError()), 5_000);
        }),
      ]);
    } finally {
      if (timeout !== undefined) clearTimeout(timeout);
    }
    await this.assertClean();
    if (!this.exactRequest) throw new ExactSubmitEvidenceError();
  }

  async waitForExactResponseStatus(): Promise<number> {
    await this.waitForExactRequest();
    const request = this.exactRequest;
    if (!request) throw new ExactSubmitEvidenceError();
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      const response = await Promise.race([
        request.response(),
        new Promise<never>((_resolve, reject) => {
          timeout = setTimeout(() => reject(new Error("submit response timeout")), 20_000);
        }),
      ]);
      const status = response?.status();
      if (!isSuccessfulExactSubmitHttpStatus(status)) {
        throw new Error("submit response status");
      }
      return status;
    } finally {
      if (timeout !== undefined) clearTimeout(timeout);
    }
  }

  async assertClean(): Promise<void> {
    if (this.violation) throw this.violation;
    if (this.closed) throw new ExactSubmitEvidenceError();
  }

  async handleRoute(route: Route): Promise<void> {
    if (this.violation) {
      await route.abort("blockedbyclient");
      return;
    }
    const request = route.request();
    const method = request.method().toLowerCase();
    const safeMethod = method === "get" || method === "head" || method === "options";
    const navigation = request.isNavigationRequest();
    const sourceMainFrame = this.isSourceMainFrameRequest(request);

    if (request.serviceWorker()) {
      await this.block(route);
      return;
    }

    if (this.phase === "installed") {
      if (!safeMethod) {
        await this.block(route);
        return;
      }
      if (navigation && (!sourceMainFrame || !this.isApprovedPreFillNavigation(request.url()))) {
        await this.block(route);
        return;
      }
      await route.fallback();
      return;
    }

    if (this.phase === "begun") {
      if (safeMethod && !navigation) {
        await route.abort("blockedbyclient");
        return;
      }
      await this.block(route);
      return;
    }

    if (this.phase !== "armed" || !this.expectation) {
      await this.block(route);
      return;
    }

    if (this.exactRequest) {
      if (safeMethod && !navigation) {
        await route.abort("blockedbyclient");
        return;
      }
      if (safeMethod
        && !request.serviceWorker()
        && navigation
        && sourceMainFrame
        && this.isExactSubmitRedirect(request)) {
        await route.fallback();
        return;
      }
      await this.block(route);
      return;
    }

    if (safeMethod && !navigation) {
      await route.abort("blockedbyclient");
      return;
    }

    if (safeMethod || !navigation || !sourceMainFrame) {
      await this.block(route);
      return;
    }

    const allHeaders = await request.allHeaders().catch(() => undefined);
    if (!allHeaders) {
      await this.block(route);
      return;
    }
    const outgoing = {
      url: request.url(),
      method,
      contentType: allHeaders["content-type"] ?? "",
      body: request.postDataBuffer(),
      headers: allHeaders,
    };
    let hydratedBody: Buffer | undefined;
    try {
      if (this.ownedRequest) {
        hydratedBody = hydrateExactOutgoingSubmit(
          this.expectation,
          this.ownedRequest,
          outgoing,
        );
      } else {
        assertExactOutgoingSubmit(this.expectation, outgoing);
      }
    } catch {
      await this.block(route);
      return;
    }

    this.exactRequest = request;
    try {
      if (hydratedBody) {
        await route.fallback({ postData: hydratedBody });
      } else {
        await route.fallback();
      }
    } catch (error) {
      this.recordViolation(new AdditionalSubmitRequestError());
      throw error;
    }
    this.resolveExactRequest?.();
  }

  blockExternalEffect(): void {
    this.recordViolation(this.exactRequest
      ? new AdditionalSubmitRequestError()
      : new ExactSubmitEvidenceError());
  }

  private async block(route: Route): Promise<void> {
    this.blockExternalEffect();
    await route.abort("blockedbyclient");
  }

  private recordViolation(error: Error): void {
    if (this.violation) return;
    this.violation = error;
    this.rejectExactRequest?.(error);
  }

  private isSourceMainFrameRequest(request: Request): boolean {
    try {
      return request.frame() === this.page.mainFrame();
    } catch {
      return false;
    }
  }

  private isApprovedPreFillNavigation(rawUrl: string): boolean {
    if (!this.adapter || !this.approvedJobKey) return false;
    try {
      try {
        return certifiedProviderJobKey(this.adapter, rawUrl, "submit") === this.approvedJobKey;
      } catch {
        return certifiedProviderJobKey(this.adapter, rawUrl, "confirmation") === this.approvedJobKey;
      }
    } catch {
      return false;
    }
  }

  private isExactSubmitRedirect(request: Request): boolean {
    if (!this.isApprovedPreFillNavigation(request.url())) return false;
    let redirectedFrom = request.redirectedFrom();
    while (redirectedFrom) {
      if (redirectedFrom === this.exactRequest) return true;
      redirectedFrom = redirectedFrom.redirectedFrom();
    }
    return false;
  }

  private async assertNoServiceWorker(): Promise<void> {
    const contextHasServiceWorker = this.page.context().serviceWorkers().length > 0;
    const pageIsControlled = await this.page.evaluate(() => (
      Boolean(navigator.serviceWorker?.controller)
    )).catch(() => true);
    if (contextHasServiceWorker || pageIsControlled) {
      this.blockExternalEffect();
      throw this.violation;
    }
  }

  private validateArmedSnapshot(
    expectation: ExactSubmitExpectation,
    snapshot: CertifiedSubmitFormSnapshot,
  ): OwnedExactSubmitPart[] | undefined {
    if (!this.adapter
      || !this.approvedJobKey
      || !sameSubmitTarget(snapshot.target, expectation.target)) {
      throw new ExactSubmitEvidenceError();
    }
    const approvedProviderJobId = this.approvedJobKey.split(":").at(-1);
    if (!approvedProviderJobId) throw new ExactSubmitEvidenceError();
    const fields: ExactSubmitFieldEvidence[] = [];
    const files: Array<{
      fieldName: string;
      name: string;
      byteLength: number;
      sha256: string;
    }> = [];
    const ownedParts: OwnedExactSubmitPart[] = [];
    const partOrder: ExactSubmitPartOrderEntry[] = [];
    let fieldIndex = 0;
    let fileIndex = 0;
    const hiddenNames = new Set<string>();
    let canOwnEveryFile = true;
    for (const part of snapshot.parts) {
      if (part.kind === "field") {
        if (part.source === "submitter") throw new ExactSubmitEvidenceError();
        if (part.source === "hidden") {
          if (hiddenNames.has(part.fieldName)) throw new ExactSubmitEvidenceError();
          hiddenNames.add(part.fieldName);
          assertCertifiedHiddenField(
            this.adapter,
            part.fieldName,
            part.value,
            approvedProviderJobId,
          );
        }
        const value = normalizeMultipartTextValue(part.value);
        const valueBytes = Buffer.from(value, "utf8");
        fields.push({
          fieldName: part.fieldName,
          valueByteLength: valueBytes.byteLength,
          valueSha256: createHash("sha256").update(valueBytes).digest("hex"),
        });
        partOrder.push({ kind: "field", index: fieldIndex });
        fieldIndex += 1;
        ownedParts.push({ kind: "field", fieldName: part.fieldName, value });
        continue;
      }
      files.push({
        fieldName: part.fieldName,
        name: part.name,
        byteLength: part.byteLength,
        sha256: part.sha256,
      });
      partOrder.push({ kind: "file", index: fileIndex });
      fileIndex += 1;
      const document = this.selectedDocuments.get(selectedDocumentKey(part.name, part.sha256));
      if (!document) {
        canOwnEveryFile = false;
        continue;
      }
      if (document.byteLength !== part.byteLength) throw new ExactSubmitEvidenceError();
      ownedParts.push({
        kind: "file",
        fieldName: part.fieldName,
        name: part.name,
        mediaType: "application/pdf",
        bytes: document.bytes,
      });
    }
    if (!sameSubmitFiles(files, expectation.files)
      || !sameSubmitFields(fields, expectation.fields)
      || !sameSubmitPartOrder(partOrder, expectation.partOrder)) {
      throw new ExactSubmitEvidenceError();
    }
    return canOwnEveryFile ? ownedParts : undefined;
  }
}

class PlaywrightExactSubmitContextGuard {
  private active?: PlaywrightExactSubmitGuard;
  private installPromise?: Promise<void>;

  constructor(private readonly context: BrowserContext) {}

  async activate(guard: PlaywrightExactSubmitGuard): Promise<void> {
    if (this.active && this.active !== guard) throw new ExactSubmitEvidenceError();
    if (!this.installPromise) {
      this.installPromise = this.install();
    }
    await this.installPromise;
    this.active = guard;
  }

  deactivate(guard: PlaywrightExactSubmitGuard): void {
    if (this.active === guard) this.active = undefined;
  }

  private async install(): Promise<void> {
    this.context.on("page", (page) => {
      const active = this.active;
      if (!active || page === active.page) return;
      active.blockExternalEffect();
      void page.close().catch(() => undefined);
    });
    this.context.on("serviceworker", () => {
      this.active?.blockExternalEffect();
    });
    await this.context.route("**/*", async (route) => {
      const active = this.active;
      if (!active) {
        await route.fallback();
        return;
      }
      await active.handleRoute(route);
    });
    await this.context.routeWebSocket(/.*/u, async (webSocket: WebSocketRoute) => {
      const active = this.active;
      active?.blockExternalEffect();
      await webSocket.close({ code: 1008, reason: "Application submission boundary" });
    });
  }
}

function normalizeMultipartTextValue(value: string): string {
  return value.replace(/\r\n|\r|\n/gu, "\r\n");
}

interface SelectedSubmitDocument extends FormFileEvidence {
  bytes: Buffer;
}

function selectedDocumentKey(name: string, sha256: string): string {
  return `${name}\u0000${sha256}`;
}

function certifiedAdapterFromProviderJobKey(
  providerJobKey: string,
): CertifiedFinalSubmitAdapter {
  if (providerJobKey.startsWith("greenhouse:")) return "greenhouse";
  if (providerJobKey.startsWith("lever:")) return "lever";
  throw new Error("Browser could not identify the application submit fields");
}

function assertCertifiedHiddenField(
  adapter: CertifiedFinalSubmitAdapter,
  fieldName: string,
  value: string,
  approvedProviderJobId: string,
): void {
  const jobField = adapter === "greenhouse"
    ? /^(?:job_id|jobid|gh_jid|posting_id|postingid|token|job_application\[(?:job_id|jobid|gh_jid|posting_id|postingid|token)\])$/iu
    : /^(?:posting_id|postingid)$/iu;
  if (jobField.test(fieldName)) {
    if (value !== approvedProviderJobId) throw new ExactSubmitEvidenceError();
    return;
  }
  const opaqueFields = adapter === "greenhouse"
    ? new Set(["authenticity_token", "utf8", "source", "ccuid"])
    : new Set(["_csrf", "csrf", "lever-source"]);
  if (!opaqueFields.has(fieldName)) throw new ExactSubmitEvidenceError();
  if ((fieldName === "authenticity_token" || fieldName === "_csrf" || fieldName === "csrf")
    && value.length < 1) {
    throw new ExactSubmitEvidenceError();
  }
  if (fieldName === "utf8" && value !== "✓") throw new ExactSubmitEvidenceError();
}

function sameSubmitTarget(
  actual: EffectiveSubmitTargetIdentity,
  expected: EffectiveSubmitTargetIdentity,
): boolean {
  return actual.actionUrl === expected.actionUrl
    && actual.method === expected.method
    && actual.enctype === expected.enctype
    && actual.formTarget === expected.formTarget
    && actual.providerJobKey === expected.providerJobKey
    && actual.formIdentity === expected.formIdentity;
}

function sameSubmitFiles(
  actual: readonly ExactSubmitFileEvidence[],
  expected: readonly Readonly<ExactSubmitFileEvidence>[],
): boolean {
  return actual.length === expected.length && actual.every((file, index) => {
    const item = expected[index];
    return file.fieldName === item?.fieldName
      && file.name === item.name
      && file.byteLength === item.byteLength
      && file.sha256 === item.sha256;
  });
}

function sameSubmitFields(
  actual: readonly ExactSubmitFieldEvidence[],
  expected: readonly Readonly<ExactSubmitFieldEvidence>[],
): boolean {
  return actual.length === expected.length && actual.every((field, index) => {
    const item = expected[index];
    return field.fieldName === item?.fieldName
      && field.valueByteLength === item.valueByteLength
      && field.valueSha256 === item.valueSha256;
  });
}

function sameSubmitPartOrder(
  actual: readonly ExactSubmitPartOrderEntry[],
  expected: readonly Readonly<ExactSubmitPartOrderEntry>[],
): boolean {
  return actual.length === expected.length && actual.every((entry, index) => {
    const item = expected[index];
    return entry.kind === item?.kind && entry.index === item.index;
  });
}

export function formControlKind(value: string): FormControlKind {
  return ["text", "email", "tel", "url", "textarea", "select", "checkbox", "radio", "file", "hidden"]
    .includes(value) ? value as FormControlKind : "other";
}
