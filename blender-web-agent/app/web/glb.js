/* glb.js — minimal GLB reader + wireframe preview.
 *
 * This draws REAL geometry that Blender exported, decoded from the GLB's own
 * accessors. It is deliberately not a Blender viewport: no shading, no PBR, no
 * interaction with the runtime. It exists so the workspace is useful while the
 * interactive WebGPU viewport is unavailable.
 */
(function (global) {
  'use strict';

  const COMPONENT = {
    5120: { size: 1, get: (dv, o) => dv.getInt8(o), norm: (v) => v / 127 },
    5121: { size: 1, get: (dv, o) => dv.getUint8(o), norm: (v) => v / 255 },
    5122: { size: 2, get: (dv, o) => dv.getInt16(o, true), norm: (v) => v / 32767 },
    5123: { size: 2, get: (dv, o) => dv.getUint16(o, true), norm: (v) => v / 65535 },
    5125: { size: 4, get: (dv, o) => dv.getUint32(o, true), norm: (v) => v },
    5126: { size: 4, get: (dv, o) => dv.getFloat32(o, true), norm: (v) => v },
  };
  const NCOMP = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 };

  function parseGLB(buffer) {
    const dv = new DataView(buffer);
    if (dv.getUint32(0, true) !== 0x46546c67) throw new Error('not a GLB');
    const jsonLen = dv.getUint32(12, true);
    const json = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, 20, jsonLen)));
    const binStart = 20 + jsonLen + 8;
    const binLen = dv.getUint32(20 + jsonLen, true);
    const bin = new DataView(buffer, binStart, Math.min(binLen, buffer.byteLength - binStart));

    const readAccessor = (i) => {
      const acc = json.accessors[i];
      const bv = json.bufferViews[acc.bufferView];
      const comp = COMPONENT[acc.componentType];
      const n = NCOMP[acc.type];
      const stride = bv.byteStride || comp.size * n;
      const base = (bv.byteOffset || 0) + (acc.byteOffset || 0);
      const out = new Float32Array(acc.count * n);
      for (let k = 0; k < acc.count; k++) {
        for (let c = 0; c < n; c++) {
          const v = comp.get(bin, base + k * stride + c * comp.size);
          out[k * n + c] = acc.normalized ? comp.norm(v) : v;
        }
      }
      return { data: out, n, count: acc.count };
    };

    const materials = (json.materials || []).map((m) => {
      const p = (m.pbrMetallicRoughness && m.pbrMetallicRoughness.baseColorFactor) || [0.8, 0.8, 0.8, 1];
      return { name: m.name || 'material', color: p.slice(0, 3) };
    });

    const meshes = [];
    for (const node of json.nodes || []) {
      if (node.mesh == null) continue;
      const mesh = json.meshes[node.mesh];
      const t = node.translation || [0, 0, 0];
      const s = node.scale || [1, 1, 1];
      for (const prim of mesh.primitives || []) {
        const pos = readAccessor(prim.attributes.POSITION);
        let idx = null;
        if (prim.indices != null) idx = readAccessor(prim.indices).data;
        // apply the node's translation + scale so the assembly reads correctly
        const verts = new Float32Array(pos.count * 3);
        for (let k = 0; k < pos.count; k++) {
          verts[k * 3] = pos.data[k * 3] * s[0] + t[0];
          verts[k * 3 + 1] = pos.data[k * 3 + 1] * s[1] + t[1];
          verts[k * 3 + 2] = pos.data[k * 3 + 2] * s[2] + t[2];
        }
        meshes.push({
          name: node.name || mesh.name || 'mesh',
          verts,
          indices: idx,
          material: prim.material != null ? materials[prim.material] : null,
        });
      }
    }
    return { meshes, materials, generator: (json.asset || {}).generator || 'unknown' };
  }

  function bounds(model) {
    const lo = [Infinity, Infinity, Infinity];
    const hi = [-Infinity, -Infinity, -Infinity];
    for (const m of model.meshes) {
      for (let k = 0; k < m.verts.length; k += 3) {
        for (let c = 0; c < 3; c++) {
          const v = m.verts[k + c];
          if (v < lo[c]) lo[c] = v;
          if (v > hi[c]) hi[c] = v;
        }
      }
    }
    if (!isFinite(lo[0])) return null;
    const ctr = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2];
    const size = Math.max(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]) || 1;
    return { lo, hi, center: ctr, size };
  }

  /* Orthographic-ish projection with a fixed 3/4 view; the model is centred and
     scaled to fit. Edges are drawn from real triangles (deduplicated). */
  function render(canvas, model, opts) {
    const o = Object.assign({ mode: 'wire', yaw: -0.62, pitch: 0.42, fit: null }, opts || {});
    const ctx = canvas.getContext('2d');
    const dpr = Math.min(global.devicePixelRatio || 1, 2);
    const w = canvas.clientWidth || canvas.width;
    const h = canvas.clientHeight || canvas.height;
    if (canvas.width !== Math.floor(w * dpr) || canvas.height !== Math.floor(h * dpr)) {
      canvas.width = Math.floor(w * dpr);
      canvas.height = Math.floor(h * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    if (!model || !model.meshes.length) return { drawn: 0 };

    const b = o.fit || bounds(model);
    if (!b) return { drawn: 0 };
    const scale = (Math.min(w, h) * 0.62) / b.size;
    const cy = Math.cos(o.yaw), sy = Math.sin(o.yaw);
    const cp = Math.cos(o.pitch), sp = Math.sin(o.pitch);

    const project = (x, y, z) => {
      x -= b.center[0]; y -= b.center[1]; z -= b.center[2];
      const x1 = x * cy - y * sy;
      const y1 = x * sy + y * cy;
      const y2 = y1 * cp - z * sp;
      const z2 = y1 * sp + z * cp;
      return [w / 2 + x1 * scale, h / 2 - z2 * scale, y2];
    };

    let edges = 0;
    const MAX_EDGES = 60000;

    if (o.mode === 'points') {
      for (const m of model.meshes) {
        ctx.fillStyle = m.material ? rgb(m.material.color) : '#8fb4d9';
        for (let k = 0; k < m.verts.length; k += 3) {
          const p = project(m.verts[k], m.verts[k + 1], m.verts[k + 2]);
          ctx.fillRect(p[0], p[1], 1.6, 1.6);
        }
      }
      return { drawn: model.meshes.reduce((a, m) => a + m.verts.length / 3, 0) };
    }

    for (const m of model.meshes) {
      const v = m.verts;
      const line = m.material ? rgba(m.material.color, 0.85) : 'rgba(150,180,210,.8)';
      ctx.strokeStyle = line;
      ctx.lineWidth = 1;
      ctx.beginPath();
      const seen = new Set();
      const emit = (a, b2) => {
        if (edges > MAX_EDGES) return;
        const key = a < b2 ? a * 4294967296 + b2 : b2 * 4294967296 + a;
        if (seen.has(key)) return;
        seen.add(key);
        const p1 = project(v[a * 3], v[a * 3 + 1], v[a * 3 + 2]);
        const p2 = project(v[b2 * 3], v[b2 * 3 + 1], v[b2 * 3 + 2]);
        ctx.moveTo(p1[0], p1[1]);
        ctx.lineTo(p2[0], p2[1]);
        edges++;
      };
      if (m.indices) {
        for (let k = 0; k + 2 < m.indices.length; k += 3) {
          const a = m.indices[k], b2 = m.indices[k + 1], c = m.indices[k + 2];
          emit(a, b2); emit(b2, c); emit(c, a);
        }
      } else {
        for (let k = 0; k + 2 < v.length / 3; k += 3) {
          const a = k, b2 = k + 1, c = k + 2;
          emit(a, b2); emit(b2, c); emit(c, a);
        }
      }
      ctx.stroke();
    }
    return { drawn: edges };
  }

  const rgb = (c) => 'rgb(' + c.map((x) => Math.round(Math.min(1, Math.max(0, x)) * 255)).join(',') + ')';
  const rgba = (c, a) => 'rgba(' + c.map((x) => Math.round(Math.min(1, Math.max(0, x)) * 255)).join(',') + ',' + a + ')';

  global.BWGLB = { parseGLB, bounds, render };
})(window);
