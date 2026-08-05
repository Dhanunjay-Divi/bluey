import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import type {
  ExactSubmitExpectation,
  ExactSubmitFieldEvidence,
  ExactSubmitFileEvidence,
} from "../src/contracts.js";
import {
  assertExactOutgoingSubmit,
  assertExactOutgoingSubmitShape,
  createOwnedExactSubmitRequest,
  ExactSubmitEvidenceError,
  hydrateExactOutgoingSubmit,
} from "../src/trusted-submit.js";

const ACTION_URL = "https://jobs.lever.co/acme/11111111-1111-4111-8111-111111111111/apply";
const BOUNDARY = "----bluey-exact-submit-boundary";

describe("trusted outgoing submit evidence", () => {
  it("accepts the exact target and Node-hashed multipart file bytes", () => {
    const resume = selectedFile("resume", "approved resume bytes");
    const expectation = submitExpectation([resume]);

    expect(() => assertExactOutgoingSubmit(expectation, request([
      multipartFile(resume, Buffer.from("approved resume bytes")),
    ]))).not.toThrow();
  });

  it("rejects same-name request bytes even if DOM hashing was forged", () => {
    const resume = selectedFile("resume", "approved resume bytes");
    const forgedBytes = Buffer.from("malicious resume byte");
    expect(forgedBytes.byteLength).toBe(resume.byteLength);

    expect(() => assertExactOutgoingSubmit(submitExpectation([resume]), request([
      multipartFile(resume, forgedBytes),
    ]))).toThrow(ExactSubmitEvidenceError);
  });

  it.each([
    ["extra file", (files: ExactSubmitFileEvidence[]) => [
      multipartFile(files[0]!, Buffer.from("approved resume bytes")),
      multipartFile(selectedFile("coverLetter", "unexpected cover bytes"), Buffer.from("unexpected cover bytes")),
    ]],
    ["reordered files", (files: ExactSubmitFileEvidence[]) => [
      multipartFile(files[1]!, Buffer.from("approved cover bytes")),
      multipartFile(files[0]!, Buffer.from("approved resume bytes")),
    ]],
  ] as const)("rejects %s in the actual multipart request", (_name, requestParts) => {
    const files = [
      selectedFile("resume", "approved resume bytes"),
      selectedFile("coverLetter", "approved cover bytes", "cover-letter"),
    ];

    expect(() => assertExactOutgoingSubmit(
      submitExpectation(_name === "extra file" ? [files[0]!] : files),
      request(requestParts(files)),
    )).toThrow(ExactSubmitEvidenceError);
  });

  it.each([
    ["wrong URL", { url: `${ACTION_URL}?changed=1` }],
    ["wrong method", { method: "GET" }],
    ["non-multipart body", { contentType: "application/x-www-form-urlencoded" }],
  ] as const)("rejects %s before the request is released", (_name, overrides) => {
    const resume = selectedFile("resume", "approved resume bytes");

    expect(() => assertExactOutgoingSubmit(
      submitExpectation([resume]),
      request([multipartFile(resume, Buffer.from("approved resume bytes"))], overrides),
    )).toThrow(ExactSubmitEvidenceError);
  });

  it.each([
    ["unquoted filename", (file: ExactSubmitFileEvidence) => (
      `name="${file.fieldName}"; filename=${file.name}`
    )],
    ["extended filename", (file: ExactSubmitFileEvidence) => (
      `name="${file.fieldName}"; filename*=UTF-8''${file.name}`
    )],
    ["duplicate filename", (file: ExactSubmitFileEvidence) => (
      `name="${file.fieldName}"; filename="${file.name}"; filename="${file.name}"`
    )],
    ["duplicate name", (file: ExactSubmitFileEvidence) => (
      `name="${file.fieldName}"; name="${file.fieldName}"; filename="${file.name}"`
    )],
    ["quoted plus extended filename", (file: ExactSubmitFileEvidence) => (
      `name="${file.fieldName}"; filename="${file.name}"; filename*=UTF-8''${file.name}`
    )],
  ] as const)("strictly rejects a %s multipart parameter", (_name, disposition) => {
    const resume = selectedFile("resume", "approved resume bytes");

    expect(() => assertExactOutgoingSubmit(
      submitExpectation([resume]),
      request([multipartPart(disposition(resume), Buffer.from("approved resume bytes"))]),
    )).toThrow(ExactSubmitEvidenceError);
  });

  it.each([
    `multipart/form-data; boundary=${BOUNDARY}; boundary=other`,
    `multipart/form-data; boundary*=UTF-8''${BOUNDARY}`,
    `multipart/form-data; boundary=${BOUNDARY}; charset=utf-8`,
    `multipart/form-data; boundary="${BOUNDARY}"; boundary=${BOUNDARY}`,
  ])("rejects ambiguous Content-Type parameters: %s", (contentType) => {
    const resume = selectedFile("resume", "approved resume bytes");
    expect(() => assertExactOutgoingSubmit(
      submitExpectation([resume]),
      request([multipartFile(resume, Buffer.from("approved resume bytes"))], { contentType }),
    )).toThrow(ExactSubmitEvidenceError);
  });

  it.each([
    ["wrong file MIME", "Content-Type: application/octet-stream"],
    ["duplicate file MIME", "Content-Type: application/pdf\r\nContent-Type: application/pdf"],
    ["transfer encoding", "Content-Type: application/pdf\r\nContent-Transfer-Encoding: binary"],
  ])("rejects %s ambiguity in a file part", (_name, headers) => {
    const resume = selectedFile("resume", "approved resume bytes");
    const part = Buffer.concat([
      Buffer.from(
        `--${BOUNDARY}\r\nContent-Disposition: form-data; name="${resume.fieldName}"; `
          + `filename="${resume.name}"\r\n${headers}\r\n\r\n`,
        "ascii",
      ),
      Buffer.from("approved resume bytes"),
      Buffer.from("\r\n", "ascii"),
    ]);
    expect(() => assertExactOutgoingSubmit(
      submitExpectation([resume]),
      request([part]),
    )).toThrow(ExactSubmitEvidenceError);
  });

  it.each([
    ["changed approved answer", [multipartText("email", "mallory@example.com")]],
    ["duplicate approved field", [
      multipartText("email", "ada@example.com"),
      multipartText("email", "ada@example.com"),
    ]],
    ["method override field", [multipartText("_method", "PUT")]],
  ] as const)("rejects %s in non-file multipart evidence", (_name, textParts) => {
    const resume = selectedFile("resume", "approved resume bytes");
    const exactFile = multipartFile(resume, Buffer.from("approved resume bytes"));
    expect(() => assertExactOutgoingSubmit(
      submitExpectation([resume]),
      requestWithExactParts([...textParts, exactFile]),
    )).toThrow(ExactSubmitEvidenceError);
  });

  it("rejects a same-action hidden job_id for another provider job", () => {
    const resume = selectedFile("resume", "approved resume bytes");
    const approvedJobId = "11111111-1111-4111-8111-111111111111";
    const expected = submitExpectation([resume], [
      selectedField("email", "ada@example.com"),
      selectedField("job_id", approvedJobId),
    ]);
    expect(() => assertExactOutgoingSubmit(expected, requestWithExactParts([
      multipartText("email", "ada@example.com"),
      multipartText("job_id", "22222222-2222-4222-8222-222222222222"),
      multipartFile(resume, Buffer.from("approved resume bytes")),
    ]))).toThrow(ExactSubmitEvidenceError);
  });

  it("rejects field names shared across file and text parts", () => {
    const resume = selectedFile("payload", "approved resume bytes");
    const expected = submitExpectation([resume], [selectedField("payload", "text")]);

    expect(() => assertExactOutgoingSubmit(expected, requestWithExactParts([
      multipartText("payload", "text"),
      multipartFile(resume, Buffer.from("approved resume bytes")),
    ]))).toThrow(ExactSubmitEvidenceError);
  });

  it("owns full upload bytes while binding the browser's global multipart order", () => {
    const fileBytes = Buffer.from("approved resume bytes");
    const resume = selectedFile("resume", fileBytes.toString("utf8"));
    const expected = submitExpectation([resume]);
    const owned = createOwnedExactSubmitRequest(expected, [{
      kind: "field",
      fieldName: "email",
      value: "ada@example.com",
    }, {
      kind: "file",
      fieldName: "resume",
      name: resume.name,
      mediaType: "application/pdf",
      bytes: fileBytes,
    }]);
    const omittedFileBody = requestWithExactParts([
      multipartText("email", "ada@example.com"),
      multipartFile(resume, Buffer.alloc(0)),
    ]);

    const expectedLength = omittedFileBody.body.byteLength + fileBytes.byteLength;
    const hydrated = hydrateExactOutgoingSubmit(expected, owned, {
      ...omittedFileBody,
      headers: { "content-length": String(expectedLength) },
    });
    expect(hydrated.byteLength).toBe(expectedLength);
    expect(() => assertExactOutgoingSubmit(expected, {
      url: ACTION_URL,
      method: "POST",
      contentType: omittedFileBody.contentType,
      body: hydrated,
    })).not.toThrow();
    expect(() => assertExactOutgoingSubmitShape(expected, owned, requestWithExactParts([
      multipartFile(resume, Buffer.alloc(0)),
      multipartText("email", "ada@example.com"),
    ]))).toThrow(ExactSubmitEvidenceError);
  });

  it("rejects hydration when the browser Content-Length does not match", () => {
    const fileBytes = Buffer.from("approved resume bytes");
    const resume = selectedFile("resume", fileBytes.toString("utf8"));
    const expected = submitExpectation([resume]);
    const owned = createOwnedExactSubmitRequest(expected, [{
      kind: "field",
      fieldName: "email",
      value: "ada@example.com",
    }, {
      kind: "file",
      fieldName: "resume",
      name: resume.name,
      mediaType: "application/pdf",
      bytes: fileBytes,
    }]);
    const observed = requestWithExactParts([
      multipartText("email", "ada@example.com"),
      multipartFile(resume, Buffer.alloc(0)),
    ]);

    expect(() => hydrateExactOutgoingSubmit(expected, owned, {
      ...observed,
      headers: { "content-length": String(observed.body.byteLength) },
    })).toThrow(ExactSubmitEvidenceError);
  });
});

function selectedFile(
  fieldName: string,
  content: string,
  kind: "resume" | "cover-letter" = "resume",
): ExactSubmitFileEvidence {
  const bytes = Buffer.from(content);
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  return {
    fieldName,
    name: `${kind}-${sha256}.pdf`,
    byteLength: bytes.byteLength,
    sha256,
  };
}

function submitExpectation(
  files: ExactSubmitFileEvidence[],
  fields: ExactSubmitFieldEvidence[] = [selectedField("email", "ada@example.com")],
): ExactSubmitExpectation {
  return {
    target: {
      actionUrl: ACTION_URL,
      method: "post",
      enctype: "multipart/form-data",
      formTarget: "_self",
      providerJobKey: "lever:jobs.lever.co:acme:11111111-1111-4111-8111-111111111111",
      formIdentity: "[0,\"application-form\"]",
    },
    files,
    fields,
    partOrder: [
      ...fields.map((_field, index) => ({ kind: "field" as const, index })),
      ...files.map((_file, index) => ({ kind: "file" as const, index })),
    ],
  };
}

function selectedField(fieldName: string, value: string): ExactSubmitFieldEvidence {
  const bytes = Buffer.from(value, "utf8");
  return {
    fieldName,
    valueByteLength: bytes.byteLength,
    valueSha256: createHash("sha256").update(bytes).digest("hex"),
  };
}

function multipartFile(file: ExactSubmitFileEvidence, bytes: Buffer): Buffer {
  return multipartPart(
    `name="${file.fieldName}"; filename="${file.name}"`,
    bytes,
  );
}

function multipartPart(disposition: string, bytes: Buffer): Buffer {
  return Buffer.concat([
    Buffer.from(
      `--${BOUNDARY}\r\nContent-Disposition: form-data; ${disposition}\r\n`
        + "Content-Type: application/pdf\r\n\r\n",
      "ascii",
    ),
    bytes,
    Buffer.from("\r\n", "ascii"),
  ]);
}

function request(
  parts: Buffer[],
  overrides: Partial<{
    url: string;
    method: string;
    contentType: string;
  }> = {},
) {
  return {
    url: overrides.url ?? ACTION_URL,
    method: overrides.method ?? "POST",
    contentType: overrides.contentType ?? `multipart/form-data; boundary=${BOUNDARY}`,
    body: Buffer.concat([
      multipartText("email", "ada@example.com"),
      ...parts,
      Buffer.from(`--${BOUNDARY}--\r\n`, "ascii"),
    ]),
  };
}

function requestWithExactParts(parts: Buffer[]) {
  return {
    url: ACTION_URL,
    method: "POST",
    contentType: `multipart/form-data; boundary=${BOUNDARY}`,
    body: Buffer.concat([...parts, Buffer.from(`--${BOUNDARY}--\r\n`, "ascii")]),
  };
}

function multipartText(fieldName: string, value: string): Buffer {
  return Buffer.from(
    `--${BOUNDARY}\r\nContent-Disposition: form-data; name="${fieldName}"\r\n\r\n`
      + `${value}\r\n`,
    "utf8",
  );
}
