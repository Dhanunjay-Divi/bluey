import { afterEach, describe, expect, it, vi } from "vitest";
import { safeDownloadFileName, saveDownloadedBlob } from "./download";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("authenticated evidence download", () => {
  it("uses only a safe leaf filename", () => {
    expect(safeDownloadFileName("../../private/receipt.json")).toBe("receipt.json");
    expect(safeDownloadFileName("..\\..\\confirmation?.png")).toBe("confirmation-.png");
    expect(safeDownloadFileName("...", "evidence.bin")).toBe("evidence.bin");
  });

  it("clicks a temporary download link and always revokes its object URL", () => {
    const click = vi.fn();
    const remove = vi.fn();
    const appendChild = vi.fn();
    const link = {
      href: "",
      download: "",
      rel: "",
      click,
      remove,
    } as unknown as HTMLAnchorElement;
    const createObjectURL = vi.fn(() => "blob:bluey-evidence");
    const revokeObjectURL = vi.fn();
    vi.stubGlobal("URL", { createObjectURL, revokeObjectURL });
    vi.stubGlobal("document", {
      createElement: vi.fn(() => link),
      body: { appendChild },
    });

    saveDownloadedBlob(new Blob(["receipt"]), "../../application-receipt.json");

    expect(createObjectURL).toHaveBeenCalledTimes(1);
    expect(link.href).toBe("blob:bluey-evidence");
    expect(link.download).toBe("application-receipt.json");
    expect(link.rel).toBe("noopener");
    expect(appendChild).toHaveBeenCalledWith(link);
    expect(click).toHaveBeenCalledTimes(1);
    expect(remove).toHaveBeenCalledTimes(1);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:bluey-evidence");
  });

  it("revokes the object URL when triggering the browser download fails", () => {
    const remove = vi.fn();
    const revokeObjectURL = vi.fn();
    const link = {
      href: "",
      download: "",
      rel: "",
      click: vi.fn(() => {
        throw new Error("browser blocked the download");
      }),
      remove,
    } as unknown as HTMLAnchorElement;
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn(() => "blob:failed-evidence"),
      revokeObjectURL,
    });
    vi.stubGlobal("document", {
      createElement: vi.fn(() => link),
      body: { appendChild: vi.fn() },
    });

    expect(() => saveDownloadedBlob(new Blob(["receipt"]), "receipt.json")).toThrow(
      "browser blocked the download",
    );
    expect(remove).toHaveBeenCalledTimes(1);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:failed-evidence");
  });
});
