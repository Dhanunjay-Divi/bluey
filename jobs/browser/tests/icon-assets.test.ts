import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const assets = join(import.meta.dirname, "..", "assets");

describe("Bluey Browser platform icon family", () => {
  it("contains exact PNG dimensions from 16 through Retina 1024", async () => {
    for (const size of [16, 32, 48, 64, 128, 256, 512, 1024]) {
      const png = await readFile(join(assets, `icon-${size}.png`));
      expect(png.subarray(1, 4).toString("ascii")).toBe("PNG");
      expect(png.readUInt32BE(16)).toBe(size);
      expect(png.readUInt32BE(20)).toBe(size);
    }
  });

  it("ships true multi-representation ICNS/ICO and vector masters", async () => {
    const [icns, ico, vector, compact] = await Promise.all([
      readFile(join(assets, "icon.icns")),
      readFile(join(assets, "icon.ico")),
      readFile(join(assets, "icon-source.svg"), "utf8"),
      readFile(join(assets, "icon-small-source.svg"), "utf8"),
    ]);
    expect(icns.subarray(0, 4).toString("ascii")).toBe("icns");
    expect(icns.includes(Buffer.from("ic10"))).toBe(true);
    expect(icns.includes(Buffer.from("ic14"))).toBe(true);
    expect(ico.readUInt16LE(0)).toBe(0);
    expect(ico.readUInt16LE(2)).toBe(1);
    expect(ico.readUInt16LE(4)).toBeGreaterThanOrEqual(6);
    expect(vector).toContain("viewBox=\"0 0 1024 1024\"");
    expect(compact).toContain("Bluey Browser compact icon");
  });
});
