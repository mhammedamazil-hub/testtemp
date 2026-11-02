import type { PlyHeader } from "./types";

/**
 * Parse the ASCII header of a 3D Gaussian splat PLY file.
 *
 * This is used as an independent, in-browser verification of the file the
 * worker produced: a download is only offered as "verified" once its header
 * describes real vertices and the 3DGS attribute set (position + scale +
 * rotation + opacity).
 */
export function parsePlyHeader(buffer: ArrayBuffer): PlyHeader {
  const bytes = new Uint8Array(buffer);
  const maxSearch = Math.min(bytes.length, 128 * 1024);
  const marker = "end_header";
  const text = new TextDecoder("ascii", { fatal: false }).decode(bytes.subarray(0, maxSearch));
  const endIndex = text.indexOf(marker);
  if (!text.startsWith("ply") || endIndex === -1) {
    throw new Error("Not a PLY file: the header is missing.");
  }
  const headerText = text.slice(0, endIndex + marker.length);
  const lines = headerText.split(/\r?\n/);

  let format = "";
  let vertexCount = 0;
  let inVertexElement = false;
  const properties: string[] = [];

  for (const rawLine of lines) {
    const line = rawLine.trim();
    if (!line) continue;
    const parts = line.split(/\s+/);
    if (parts[0] === "format" && parts[1]) {
      format = parts.slice(1).join(" ");
    } else if (parts[0] === "element" && parts[1]) {
      inVertexElement = parts[1] === "vertex";
      if (inVertexElement && parts[2]) {
        vertexCount = Number.parseInt(parts[2], 10);
      }
    } else if (parts[0] === "property" && inVertexElement) {
      const name = parts[parts.length - 1];
      if (name) properties.push(name);
    } else if (parts[0] === "comment" || parts[0] === "obj_info") {
      continue;
    }
  }

  if (!format) throw new Error("PLY header has no format line.");
  if (!Number.isFinite(vertexCount) || vertexCount <= 0) {
    throw new Error("PLY file reports no vertices.");
  }
  const hasPosition = ["x", "y", "z"].every((axis) => properties.includes(axis));
  if (!hasPosition) throw new Error("PLY file has no x/y/z position properties.");

  return {
    format,
    vertexCount,
    properties,
    hasColor: ["f_dc_0", "f_dc_1", "f_dc_2"].every((name) => properties.includes(name)),
    headerBytes: new TextEncoder().encode(headerText + "\n").length,
  };
}

export function isGaussianSplatHeader(header: PlyHeader): boolean {
  return (
    header.properties.includes("opacity") &&
    header.properties.includes("scale_0") &&
    header.properties.includes("rot_0")
  );
}

/** Fetch just the header of a remote PLY (uses HTTP range; degrades to "unknown"). */
export async function inspectRemotePly(url: string, signal?: AbortSignal): Promise<PlyHeader | null> {
  let response: Response;
  try {
    response = await fetch(url, {
      headers: { Range: "bytes=0-65535" },
      signal,
    });
  } catch {
    return null;
  }
  if (!response.ok) return null;
  const contentRange = response.headers.get("content-range");
  const contentLength = Number(response.headers.get("content-length") ?? "0");
  // If the server ignored the Range header and the file is large, don't pull it
  // down twice just to read the header.
  if (!contentRange && contentLength > 200_000) return null;
  try {
    return parsePlyHeader(await response.arrayBuffer());
  } catch {
    return null;
  }
}
