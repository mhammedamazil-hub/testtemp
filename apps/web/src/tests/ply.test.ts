import { describe, expect, it } from "vitest";
import { isGaussianSplatHeader, parsePlyHeader } from "../lib/ply";

function headerPly(vertexCount: number, extra = "", format = "binary_little_endian 1.0"): ArrayBuffer {
  const header =
    "ply\n" +
    `format ${format}\n` +
    "comment simulated\n" +
    `element vertex ${vertexCount}\n` +
    "property float x\nproperty float y\nproperty float z\n" +
    "property float f_dc_0\nproperty float f_dc_1\nproperty float f_dc_2\n" +
    "property float opacity\n" +
    "property float scale_0\nproperty float scale_1\nproperty float scale_2\n" +
    "property float rot_0\nproperty float rot_1\nproperty float rot_2\nproperty float rot_3\n" +
    extra +
    "end_header\n";
  const bytes = new TextEncoder().encode(header);
  const buffer = new ArrayBuffer(bytes.length + 8);
  new Uint8Array(buffer).set(bytes);
  return buffer;
}

describe("parsePlyHeader", () => {
  it("reads format, count and properties", () => {
    const header = parsePlyHeader(headerPly(262_144));
    expect(header.format).toBe("binary_little_endian 1.0");
    expect(header.vertexCount).toBe(262_144);
    expect(header.properties).toContain("f_dc_0");
    expect(header.hasColor).toBe(true);
    expect(isGaussianSplatHeader(header)).toBe(true);
  });

  it("rejects non-PLY data", () => {
    const buffer = new TextEncoder().encode("hello world").buffer;
    expect(() => parsePlyHeader(buffer)).toThrow(/Not a PLY/i);
  });

  it("rejects a PLY with no vertices", () => {
    expect(() => parsePlyHeader(headerPly(0))).toThrow(/no vertices/i);
  });

  it("detects a plain point cloud without gaussian attributes", () => {
    const header =
      "ply\nformat ascii 1.0\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nend_header\n";
    const parsed = parsePlyHeader(new TextEncoder().encode(header).buffer);
    expect(parsed.vertexCount).toBe(4);
    expect(isGaussianSplatHeader(parsed)).toBe(false);
  });
});
