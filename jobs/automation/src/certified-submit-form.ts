import { Buffer } from "node:buffer";
import type { Page } from "playwright";
import type {
  CertifiedFinalSubmitAdapter,
  EffectiveSubmitTargetIdentity,
} from "./contracts.js";
import { certifiedProviderJobKey } from "./provider-job-key.js";
import { EXACT_SUBMIT_FIELD_LIMITS } from "./trusted-submit.js";
import { FORM_FILE_READBACK_LIMITS } from "./form-readback.js";

export type CertifiedSubmitFormPart =
  | {
      kind: "field";
      source: "hidden" | "submitter" | "visible";
      fieldName: string;
      value: string;
    }
  | {
      kind: "file";
      fieldName: string;
      name: string;
      byteLength: number;
      sha256: string;
      mediaType: string;
    };

export interface CertifiedSubmitFormSnapshot {
  target: EffectiveSubmitTargetIdentity;
  parts: ReadonlyArray<Readonly<CertifiedSubmitFormPart>>;
}

interface BrowserSubmitFormSnapshot {
  target: Omit<EffectiveSubmitTargetIdentity, "providerJobKey">;
  parts: CertifiedSubmitFormPart[];
}

/**
 * Inspect the certified submit form in a fresh Chromium isolated world. The
 * page cannot replace the DOM/FormData prototypes used by this inspection.
 */
export async function inspectCertifiedSubmitForm(
  page: Page,
  adapter: CertifiedFinalSubmitAdapter,
): Promise<CertifiedSubmitFormSnapshot> {
  const session = await page.context().newCDPSession(page);
  try {
    const { frameTree } = await session.send("Page.getFrameTree");
    const { executionContextId } = await session.send("Page.createIsolatedWorld", {
      frameId: frameTree.frame.id,
      worldName: "bluey-certified-submit-form",
      grantUniveralAccess: false,
    });
    const evaluated = await session.send("Runtime.evaluate", {
      expression: `(${browserInspectCertifiedSubmitForm.toString()})(${JSON.stringify(adapter)}, ${JSON.stringify({
        maxForms: 64,
        maxFieldCount: EXACT_SUBMIT_FIELD_LIMITS.maxFieldCount,
        maxFieldNameChars: EXACT_SUBMIT_FIELD_LIMITS.maxFieldNameChars,
        maxValueChars: EXACT_SUBMIT_FIELD_LIMITS.maxValueBytes,
        maxAggregateValueChars: EXACT_SUBMIT_FIELD_LIMITS.maxAggregateValueBytes,
        maxFileCount: FORM_FILE_READBACK_LIMITS.maxFileCount,
        maxFileBytes: FORM_FILE_READBACK_LIMITS.maxFileBytes,
        maxAggregateFileBytes: FORM_FILE_READBACK_LIMITS.maxAggregateBytes,
        maxFileNameChars: FORM_FILE_READBACK_LIMITS.maxFileNameChars,
        maxUrlChars: 2_048,
        maxAttributeChars: 160,
      })})`,
      contextId: executionContextId,
      awaitPromise: true,
      returnByValue: true,
    });
    if (evaluated.exceptionDetails || evaluated.result.type !== "object") {
      throw new Error("Browser could not inspect the certified application form");
    }
    const snapshot = evaluated.result.value as BrowserSubmitFormSnapshot;
    return validateBrowserSnapshot(snapshot, adapter);
  } catch {
    throw new Error("Browser could not inspect the certified application form");
  } finally {
    await session.detach().catch(() => undefined);
  }
}

function validateBrowserSnapshot(
  snapshot: BrowserSubmitFormSnapshot,
  adapter: CertifiedFinalSubmitAdapter,
): CertifiedSubmitFormSnapshot {
  if (!snapshot || typeof snapshot !== "object" || !snapshot.target
    || !Array.isArray(snapshot.parts)) {
    throw new Error("invalid snapshot");
  }
  const target = snapshot.target;
  if (typeof target.actionUrl !== "string"
    || target.actionUrl.length < 1
    || target.actionUrl.length > 2_048
    || typeof target.method !== "string"
    || !/^[a-z]{1,16}$/u.test(target.method)
    || typeof target.enctype !== "string"
    || !/^[a-z0-9!#$&^_.+\-/]{1,64}$/u.test(target.enctype)
    || typeof target.formTarget !== "string"
    || !/^[_a-z0-9-]{1,160}$/u.test(target.formTarget)
    || typeof target.formIdentity !== "string"
    || target.formIdentity.length < 1
    || target.formIdentity.length > 1_024) {
    throw new Error("invalid target");
  }
  let aggregateFieldBytes = 0;
  let aggregateFileBytes = 0;
  let fileCount = 0;
  const parts = snapshot.parts.map((part) => {
    if (!part || typeof part !== "object"
      || typeof part.fieldName !== "string"
      || !/^[A-Za-z0-9_.:[\]-]{1,240}$/u.test(part.fieldName)) {
      throw new Error("invalid part");
    }
    if (part.kind === "field") {
      if ((part.source !== "hidden" && part.source !== "submitter" && part.source !== "visible")
        || typeof part.value !== "string") {
        throw new Error("invalid field");
      }
      const valueBytes = Buffer.byteLength(normalizeMultipartTextValue(part.value), "utf8");
      aggregateFieldBytes += valueBytes;
      if (valueBytes > EXACT_SUBMIT_FIELD_LIMITS.maxValueBytes
        || aggregateFieldBytes > EXACT_SUBMIT_FIELD_LIMITS.maxAggregateValueBytes) {
        throw new Error("field bounds");
      }
      return Object.freeze({ ...part, value: normalizeMultipartTextValue(part.value) });
    }
    if (part.kind !== "file"
      || typeof part.name !== "string"
      || !/^(?:resume|cover-letter)-[a-f0-9]{64}\.pdf$/u.test(part.name)
      || typeof part.sha256 !== "string"
      || !/^[a-f0-9]{64}$/u.test(part.sha256)
      || part.name !== `resume-${part.sha256}.pdf`
        && part.name !== `cover-letter-${part.sha256}.pdf`
      || !Number.isSafeInteger(part.byteLength)
      || part.byteLength < 1
      || part.byteLength > FORM_FILE_READBACK_LIMITS.maxFileBytes
      || part.mediaType !== "application/pdf") {
      throw new Error("invalid file");
    }
    fileCount += 1;
    aggregateFileBytes += part.byteLength;
    if (fileCount > FORM_FILE_READBACK_LIMITS.maxFileCount
      || aggregateFileBytes > FORM_FILE_READBACK_LIMITS.maxAggregateBytes) {
      throw new Error("file bounds");
    }
    return Object.freeze({ ...part });
  });
  if (parts.length < 1
    || parts.length > EXACT_SUBMIT_FIELD_LIMITS.maxFieldCount
      + FORM_FILE_READBACK_LIMITS.maxFileCount) {
    throw new Error("part count");
  }
  return Object.freeze({
    target: Object.freeze({
      ...target,
      providerJobKey: certifiedProviderJobKey(adapter, target.actionUrl, "submit"),
    }),
    parts: Object.freeze(parts),
  });
}

function normalizeMultipartTextValue(value: string): string {
  return value.replace(/\r\n|\r|\n/gu, "\r\n");
}

async function browserInspectCertifiedSubmitForm(
  adapter: CertifiedFinalSubmitAdapter,
  limits: {
    maxForms: number;
    maxFieldCount: number;
    maxFieldNameChars: number;
    maxValueChars: number;
    maxAggregateValueChars: number;
    maxFileCount: number;
    maxFileBytes: number;
    maxAggregateFileBytes: number;
    maxFileNameChars: number;
    maxUrlChars: number;
    maxAttributeChars: number;
  },
): Promise<BrowserSubmitFormSnapshot> {
  const candidates = Array.from(document.querySelectorAll("button, input"))
    .filter((element): element is HTMLButtonElement | HTMLInputElement => {
      if (!(element instanceof HTMLButtonElement) && !(element instanceof HTMLInputElement)) {
        return false;
      }
      const type = element.type.toLowerCase();
      if (type !== "submit") return false;
      const form = element.form;
      if (!form) return false;
      if (adapter === "lever") {
        return form.id === "application-form"
          && (element.matches("#btn-submit[data-qa='btn-submit']")
            || element.matches("button#btn-submit.template-btn-submit")
            || element.matches(".last-section-apply button.template-btn-submit[type='submit']"));
      }
      const actionSource = element.hasAttribute("formaction")
        ? element.getAttribute("formaction") ?? ""
        : form.getAttribute("action") ?? "";
      let greenhouseAction = false;
      try {
        const host = new URL(actionSource, document.baseURI).hostname.toLowerCase();
        greenhouseAction = host === "boards.greenhouse.io"
          || host === "job-boards.greenhouse.io";
      } catch {
        greenhouseAction = false;
      }
      const text = (element instanceof HTMLInputElement ? element.value : element.textContent ?? "")
        .replace(/\s+/gu, " ")
        .trim();
      return element.id === "submit_app"
        || element.getAttribute("data-testid") === "submit-application"
        || form.id === "application_form"
        || greenhouseAction
        || /^submit application$/iu.test(text);
    });
  if (candidates.length !== 1 || document.forms.length > limits.maxForms) {
    throw new Error("Browser could not identify the certified application submit control");
  }
  const submitter = candidates[0]!;
  const form = submitter.form!;
  const formIndex = Array.from(document.forms).indexOf(form);
  if (formIndex < 0) throw new Error("Browser could not identify the certified application form");

  const actionSource = submitter.hasAttribute("formaction")
    ? submitter.getAttribute("formaction") ?? ""
    : form.getAttribute("action") ?? "";
  const methodSource = submitter.hasAttribute("formmethod")
    ? submitter.getAttribute("formmethod") ?? ""
    : form.getAttribute("method") ?? "";
  const enctypeSource = submitter.hasAttribute("formenctype")
    ? submitter.getAttribute("formenctype") ?? ""
    : form.getAttribute("enctype") ?? "";
  const targetSource = submitter.hasAttribute("formtarget")
    ? submitter.getAttribute("formtarget") ?? ""
    : form.getAttribute("target") ?? "";
  const actionUrl = new URL(actionSource, document.baseURI).href;
  const method = (methodSource.trim() || "get").toLowerCase();
  const enctype = (enctypeSource.trim() || "application/x-www-form-urlencoded").toLowerCase();
  const formTarget = (targetSource.trim() || "_self").toLowerCase();
  const attributes = [
    form.id,
    form.getAttribute("name") ?? "",
    form.getAttribute("data-qa") ?? "",
    form.getAttribute("data-testid") ?? "",
    form.getAttribute("data-greenhouse-job-id") ?? "",
    form.getAttribute("data-job-id") ?? "",
  ];
  if (actionUrl.length < 1
    || actionUrl.length > limits.maxUrlChars
    || attributes.some((value) => value.length > limits.maxAttributeChars
      || /[\u0000-\u001f\u007f]/u.test(value))) {
    throw new Error("Browser could not identify the certified application form target");
  }

  const parts: CertifiedSubmitFormPart[] = [];
  let aggregateValueChars = 0;
  let aggregateFileBytes = 0;
  let fileCount = 0;
  for (const control of Array.from(form.elements)) {
    if (control instanceof HTMLFieldSetElement || control instanceof HTMLOutputElement) continue;
    if (!(control instanceof HTMLInputElement)
      && !(control instanceof HTMLSelectElement)
      && !(control instanceof HTMLTextAreaElement)
      && !(control instanceof HTMLButtonElement)) {
      throw new Error("Browser found an unsupported form-associated control");
    }
    if (control.form !== form || control.matches(":disabled")) continue;
    const fieldName = control.getAttribute("name") ?? "";
    if (!fieldName) continue;
    if (fieldName.length > limits.maxFieldNameChars
      || /[\u0000-\u001f\u007f]/u.test(fieldName)
      || control.hasAttribute("dirname")) {
      throw new Error("Browser found an unsupported form field");
    }

    if (control instanceof HTMLButtonElement) {
      if (control !== submitter) continue;
      parts.push({ kind: "field", source: "submitter", fieldName, value: control.value });
      continue;
    }
    if (control instanceof HTMLInputElement) {
      const type = control.type.toLowerCase();
      if (["button", "reset", "image", "submit"].includes(type)) {
        if (control === submitter && type === "submit") {
          parts.push({ kind: "field", source: "submitter", fieldName, value: control.value });
        }
        continue;
      }
      if ((type === "checkbox" || type === "radio") && !control.checked) continue;
      if (type === "file") {
        const files = Array.from(control.files ?? []);
        if (files.length < 1) {
          throw new Error("Browser found an unbound empty application file field");
        }
        for (const file of files) {
          fileCount += 1;
          aggregateFileBytes += file.size;
          if (fileCount > limits.maxFileCount
            || !file.name
            || file.name.length > limits.maxFileNameChars
            || !Number.isSafeInteger(file.size)
            || file.size < 1
            || file.size > limits.maxFileBytes
            || aggregateFileBytes > limits.maxAggregateFileBytes
            || file.type !== "application/pdf") {
            throw new Error("Browser found an unsupported application file");
          }
          const bytes = await file.arrayBuffer();
          if (bytes.byteLength !== file.size || !globalThis.crypto?.subtle) {
            throw new Error("Browser could not verify an application file");
          }
          const digest = new Uint8Array(await globalThis.crypto.subtle.digest("SHA-256", bytes));
          parts.push({
            kind: "file",
            fieldName,
            name: file.name,
            byteLength: file.size,
            sha256: Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join(""),
            mediaType: file.type,
          });
        }
        continue;
      }
      parts.push({
        kind: "field",
        source: type === "hidden" ? "hidden" : "visible",
        fieldName,
        value: control.value,
      });
      continue;
    }
    if (control instanceof HTMLSelectElement) {
      for (const option of Array.from(control.selectedOptions)) {
        const parentDisabled = option.parentElement instanceof HTMLOptGroupElement
          && option.parentElement.disabled;
        if (!option.disabled && !parentDisabled) {
          parts.push({ kind: "field", source: "visible", fieldName, value: option.value });
        }
      }
      continue;
    }
    parts.push({ kind: "field", source: "visible", fieldName, value: control.value });
  }

  for (const part of parts) {
    if (part.kind !== "field") continue;
    aggregateValueChars += part.value.length;
    if (part.value.length > limits.maxValueChars
      || aggregateValueChars > limits.maxAggregateValueChars) {
      throw new Error("Browser found oversized application form values");
    }
  }
  if (parts.length < 1 || parts.length > limits.maxFieldCount + limits.maxFileCount) {
    throw new Error("Browser found an unsupported application form shape");
  }
  return {
    target: {
      actionUrl,
      method,
      enctype,
      formTarget,
      formIdentity: JSON.stringify([formIndex, ...attributes]),
    },
    parts,
  };
}
