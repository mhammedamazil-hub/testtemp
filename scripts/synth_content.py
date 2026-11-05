#!/usr/bin/env python3
"""Phase 4: synthesize audio + 2D graphical content (pure stdlib, no bpy)."""

import math
import os
import struct
import wave
import random
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
AUDIO = os.path.join(ROOT, "audio")
TEXTURES = os.path.join(ROOT, "textures")
os.makedirs(AUDIO, exist_ok=True)
os.makedirs(TEXTURES, exist_ok=True)

SR = 44100


def clamp(v, lo=0, hi=255):
    return max(lo, min(hi, int(v)))


def write_wav_stereo(path, samples_l, samples_r):
    assert len(samples_l) == len(samples_r)
    n = len(samples_l)
    header = struct.pack("<4sI4s4sIHHIIHH4sI",
                         b"RIFF", 36 + n * 4, b"WAVE", b"fmt ", 16, 1, 2, SR,
                         SR * 2 * 2, 4, 16, b"data", n * 4)
    frames = b"".join(struct.pack("<hh", l, r) for l, r in zip(samples_l, samples_r))
    with open(path, "wb") as f:
        f.write(header + frames)
    return n / SR


def write_wav_mono(path, samples):
    n = len(samples)
    header = struct.pack("<4sI4s4sIHHIIHH4sI",
                         b"RIFF", 36 + n * 2, b"WAVE", b"fmt ", 16, 1, 1, SR,
                         SR * 2, 2, 16, b"data", n * 2)
    frames = b"".join(struct.pack("<h", s) for s in samples)
    with open(path, "wb") as f:
        f.write(header + frames)
    return n / SR


def adsr(n, attack, decay, sustain, release, sr=SR):
    """Envelope, n samples."""
    total = n
    a = int(attack * sr); d = int(decay * sr); r = int(release * sr)
    s = total - a - d - r
    env = []
    for i in range(total):
        if i < a:
            env.append(i / a)
        elif i < a + d:
            t = (i - a) / d
            env.append(1 - (1 - sustain) * t)
        elif i < a + d + s:
            env.append(sustain)
        else:
            t = (i - (a + d + s)) / r
            env.append(sustain * (1 - t))
    return env


def mix_into(buf, src, gain=1.0, offset=0):
    for i, v in enumerate(src):
        j = offset + i
        if 0 <= j < len(buf):
            buf[j] = max(-1.0, min(1.0, buf[j] + v * gain))


def sine(freq, n, sr=SR):
    return [math.sin(2 * math.pi * freq * i / sr) for i in range(n)]


def noise(n, seed=0):
    r = random.Random(seed)
    return [r.uniform(-1, 1) for _ in range(n)]


def lowpass(samples, cutoff, sr=SR):
    """One-pole lowpass filter."""
    fc = math.tan(math.pi * cutoff / sr) / (1 + math.tan(math.pi * cutoff / sr))
    out = []
    prev = 0.0
    for x in samples:
        prev = prev + fc * (x - prev)
        out.append(prev)
    return out


# ---------------------------------------------------------------- SFX ------

def sfx_engine():
    """Continuous low synthetic engine hum (loopable, ~6 s)."""
    dur = 6.0
    n = int(dur * SR)
    buf_l = [0.0] * n
    buf_r = [0.0] * n
    base = 110.0  # low rumble fundamental
    for harm, amp in ((1, 0.5), (2, 0.25), (3, 0.12), (4, 0.06)):
        f = base * harm
        osc = sine(f, n)
        mix_into(buf_l, osc, amp)
        mix_into(buf_r, osc, amp * 0.9)
    # sub-rumble noise (filtered-ish via slow modulation)
    nse = noise(n, 7)
    env = adsr(n, 0.01, 0, 1, 0.05)
    # slow amplitude modulation for rev feel
    mod = [0.7 + 0.3 * math.sin(2 * math.pi * 0.4 * i / SR) for i in range(n)]
    mix_into(buf_l, nse, 0.04)
    mix_into(buf_r, nse, 0.035)
    for i in range(n):
        buf_l[i] *= env[i] * mod[i]
        buf_r[i] *= env[i] * mod[i]
    peak = max(max(map(abs, buf_l)), max(map(abs, buf_r))) or 1
    gain = 0.5 / peak
    buf_l = [clamp((x * gain + 1) * 32767.5, -32768, 32767) for x in buf_l]
    buf_r = [clamp((x * gain + 1) * 32767.5, -32768, 32767) for x in buf_r]
    return write_wav_stereo(os.path.join(AUDIO, "SFX_Car_Engine.wav"), buf_l, buf_r)


def sfx_collision():
    """Sharp high-impact 8-bit explosion crash (~0.8 s)."""
    dur = 0.8
    n = int(dur * SR)
    buf = [0.0] * n
    # impact thud: low sine burst
    thud = sine(60, n)
    env_t = adsr(n, 0.001, 0.05, 0.2, 0.3)
    mix_into(buf, thud, 0.5)
    # noise crash
    nse = noise(n, 3)
    env_n = adsr(n, 0.001, 0.1, 0.0, 0.4)
    mix_into(buf, nse, 0.9)
    # high-frequency crunch
    crunch = noise(n, 9)
    hp = [crunch[i] - crunch[i - 1] if i else 0 for i in range(n)]
    mix_into(buf, hp, 0.5)
    for i in range(n):
        buf[i] *= env_t[i] * 0.5 + env_n[i]
    peak = max(map(abs, buf)) or 1
    gain = 0.9 / peak
    return write_wav_mono(os.path.join(AUDIO, "SFX_Collision.wav"),
                          [clamp((x * gain + 1) * 32767.5, -32768, 32767) for x in buf])


def sfx_menu_select():
    """Retro electronic chime (~0.4 s)."""
    dur = 0.4
    n = int(dur * SR)
    buf = [0.0] * n
    for f, start, amp in ((880, 0, 0.9), (1320, int(0.12 * SR), 0.7)):
        seg = sine(f, n - start)
        env = adsr(len(seg), 0.01, 0.05, 0.0, 0.15)
        seg = [s * e for s, e in zip(seg, env)]
        mix_into(buf, seg, amp, start)
    peak = max(map(abs, buf)) or 1
    gain = 0.8 / peak
    return write_wav_mono(os.path.join(AUDIO, "SFX_Menu_Select.wav"),
                          [clamp((x * gain + 1) * 32767.5, -32768, 32767) for x in buf])


# ------------------------------------------------------------ PNG encoder --

def encode_png_rgba(w, h, rgba):
    rows = bytearray()
    stride = w * 4
    for y in range(h):
        rows.append(0)
        rows.extend(rgba[y * stride:(y + 1) * stride])
    compressed = zlib.compress(bytes(rows), 6)

    def chunk(tag, payload):
        c = struct.pack(">I", len(payload)) + tag + payload
        crc = zlib.crc32(tag + payload) & 0xffffffff
        return c + struct.pack(">I", crc)

    out = bytearray(b"\x89PNG\r\n\x1a\n")
    out += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    out += chunk(b"IDAT", compressed)
    out += chunk(b"IEND", b"")
    return bytes(out)


class Canvas:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.p = bytearray([0, 0, 0, 0] * w * h)

    def set(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            i = (y * self.w + x) * 4
            self.p[i:i + 4] = c

    def rect(self, x0, y0, x1, y1, c):
        for y in range(y0, y1):
            for x in range(x0, x1):
                self.set(x, y, c)

    def line(self, x0, y0, x1, y1, c):
        dx = abs(x1 - x0); dy = abs(y1 - y0)
        sx = 1 if x0 < x1 else -1; sy = 1 if y0 < y1 else -1
        err = dx - dy
        x, y = x0, y0
        while True:
            self.set(x, y, c)
            if x == x1 and y == y1:
                break
            e2 = 2 * err
            if e2 > -dy:
                err -= dy; x += sx
            if e2 < dx:
                err += dx; y += sy

    def circle(self, cx, cy, rad, c, fill=False):
        r2 = rad * rad
        for y in range(max(0, cy - rad), min(self.h, cy + rad + 1)):
            for x in range(max(0, cx - rad), min(self.w, cx + rad + 1)):
                if (x - cx) ** 2 + (y - cy) ** 2 <= r2 and (fill or abs((x - cx) ** 2 + (y - cy) ** 2 - r2) <= rad * 1.5):
                    self.set(x, y, c)

    def save(self, path):
        with open(path, "wb") as f:
            f.write(encode_png_rgba(self.w, self.h, bytes(self.p)))


# ------------------------------------------------------------- textures --

def neon_blend(canvas, x, y, w, h, center_color, edge_color):
    """Soft neon billboard rectangle with center->edge falloff."""
    for yy in range(y, y + h):
        for xx in range(x, x + w):
            fx = min(xx - x, w - 1 - xx) / max(1, w / 2)
            fy = min(yy - y, h - 1 - yy) / max(1, h / 2)
            d = min(1, fx + fy)
            cc = center_color; ec = edge_color
            c = [clamp(cc[i] + (ec[i] - cc[i]) * d) for i in range(3)]
            canvas.set(xx, yy, c + [255])


def texture_skybox():
    w, h = 512, 256
    cv = Canvas(w, h)
    rnd = random.Random(123)
    # sky gradient: deep space purple -> black
    for y in range(h // 2):
        f = y / (h // 2)
        r = clamp(20 * (1 - f)); g = clamp(5 * (1 - f)); b = clamp(40 * (1 - f) + 10 * f)
        cv.rect(0, y, w, y + 1, [r, g, b, 255])
    cv.rect(0, h // 2, w, h, [6, 6, 12, 255])
    # stars
    for _ in range(60):
        cv.set(rnd.randrange(w), rnd.randrange(h // 2), [235, 235, 245, 255])
    # city silhouette along horizon (periodic, seamlessly tileable)
    bw = 56
    for bx in range(-bw, w + bw, bw):
        height = 90 + (bx // bw % 3) * 40
        cv.rect(bx, h // 2, bx + bw - 4, h, [10, 10, 16, 255])
        # windows
        for wy in range(h // 2 + 10, h - 6, 14):
            for wx in range(bx + 4, bx + bw - 4, 8):
                if rnd.random() < 0.35:
                    cv.set(wx, wy, [40, 40, 60, 255])
    # massive neon billboards (purple / pink)
    for bx in range(40, w, 130):
        by = h // 2 + 20
        bh = 70
        bw_ = 100
        col = [200, 20, 220] if (bx // 130) % 2 == 0 else [150, 30, 255]
        neon_blend(cv, bx, by, bw_, bh, col, [max(0, c - 120) for c in col])
        # billboard frame
        cv.rect(bx, by, bx + bw_, by + 2, [255, 255, 255, 255])
        cv.rect(bx, by + bh - 2, bx + bw_, by + bh, [255, 255, 255, 255])
    cv.save(os.path.join(TEXTURES, "T_Skybox_Neon.png"))


def texture_ui_sheet():
    w, h = 512, 512
    cv = Canvas(w, h)  # transparent background (alpha 0)
    NEON_PINK = [255, 20, 220, 255]
    NEON_PURPLE = [160, 30, 255, 255]
    NEON_CYAN = [0, 240, 255, 255]
    NEON_AMBER = [255, 200, 40, 255]
    DARK = [15, 15, 24, 255]
    # speed dial: neon arc + needle, bottom center
    cx, cy, r = 256, 400, 110
    for ang in range(210, 331, 5):
        a = math.radians(ang)
        cv.set(int(cx + math.cos(a) * r), int(cy - math.sin(a) * r), NEON_PINK)
    # needle pointing ~220 deg
    a = math.radians(220)
    cv.line(cx, cy, int(cx + math.cos(a) * (r - 18)), int(cy - math.sin(a) * (r - 18)), NEON_CYAN)
    cv.circle(cx, cy, 6, NEON_CYAN, fill=True)
    cv.circle(cx, cy, 9, NEON_CYAN)
    cv.circle(cx, cy, r + 4, NEON_PINK)
    cv.circle(cx, cy, r + 8, NEON_PINK)
    # health bar border, upper left
    hx, hy, hw, hh = 40, 40, 260, 34
    cv.rect(hx, hy, hx + hw, hy + 2, NEON_CYAN)
    cv.rect(hx, hy + hh - 2, hx + hw, hy + hh, NEON_CYAN)
    cv.rect(hx, hy + 2, hx + 2, hy + hh - 2, NEON_CYAN)
    cv.rect(hx + hw - 2, hy + 2, hx + hw, hy + hh - 2, NEON_CYAN)
    cv.rect(hx + 4, hy + 4, hx + 120, hy + hh - 4, [0, 180, 120, 255])  # fill
    # 5-star wanted level grid, upper right
    sx, sy, stride = 280, 60, 42
    for s in range(5):
        star_cx = sx + s * stride
        # 5-point star outline (approx via 10 vertices)
        pts = []
        for k in range(10):
            ang = -math.pi / 2 + k * math.pi / 5
            rr = 12 if k % 2 == 0 else 5
            pts.append((star_cx + math.cos(ang) * rr, sy + math.sin(ang) * rr))
        col = NEON_AMBER if s < 3 else [90, 90, 110, 255]
        for k in range(10):
            x0, y0 = pts[k]; x1, y1 = pts[(k + 1) % 10]
            cv.line(int(x0), int(y0), int(x1), int(y1), col)
        cv.circle(star_cx, sy, 2, col)
    cv.save(os.path.join(TEXTURES, "T_UI_HUD_Sheet.png"))


# ---------------------------------------------------------------- main ----

def main():
    t_engine = sfx_engine()
    t_collision = sfx_collision()
    t_menu = sfx_menu_select()
    print(f"[audio] SFX_Car_Engine.wav   {t_engine:.2f}s")
    print(f"[audio] SFX_Collision.wav    {t_collision:.2f}s")
    print(f"[audio] SFX_Menu_Select.wav  {t_menu:.2f}s")

    texture_skybox()
    texture_ui_sheet()
    print(f"[gfx]  T_Skybox_Neon.png")
    print(f"[gfx]  T_UI_HUD_Sheet.png")

    # PCM soundtrack for MP3 encoding (written raw, encoded by Node/lamejs)
    write_soundtrack_pcm()


def write_soundtrack_pcm():
    """30 s, 110 BPM cyber-synthwave loop, mono PCM float32 raw file."""
    dur = 30.0
    n = int(dur * SR)
    bpm = 110.0
    beat = 60.0 / bpm
    buf = [0.0] * n

    def note_env(length, attack=0.02, release=0.1):
        return adsr(length, attack, 0, 0.3, release)

    # chords: Am F C G (2 bars each over 30 s -> 4 chords, ~7.5 s each)
    progression = [("Am", [55, 69.3, 82.4]), ("F", [43.7, 55, 65.9]),
                   ("C", [32.7, 41.2, 49]), ("G", [24.5, 30.9, 36.7])]
    chord_len = n // len(progression)
    for ci, (_, tones) in enumerate(progression):
        for t in tones:
            seg_start = ci * chord_len
            seg_len = chord_len
            osc = [math.sin(2 * math.pi * t * i / SR) for i in range(seg_len)]
            env = note_env(seg_len, release=0.3)
            gain = 0.14 / len(tones)
            for i in range(seg_len):
                buf[seg_start + i] += osc[i] * env[i] * gain

    # bassline: low-passed, root notes on every beat (syncopated)
    bass_pattern = [0, 0, 1, 0, 0, 1, 1, 0]  # per chord
    for ci, (_, tones) in enumerate(progression):
        root = tones[0]
        for bi in range(8):
            t0 = int((ci * 2 + bi / 4) * beat * SR)
            if not bass_pattern[bi]:
                continue
            ln = int(beat * SR * 0.55)
            osc = sine(root, ln)
            osc = lowpass(osc, 140)
            env = note_env(ln, release=0.06)
            for i in range(ln):
                if t0 + i < n:
                    buf[t0 + i] += osc[i] * env[i] * 0.5

    # drums: kick on 1, snare on 3, hats 8ths
    for bi in range(int(dur / beat)):
        t0 = int(bi * beat * SR)
        # kick
        ln = int(0.12 * SR)
        k = [math.sin(2 * math.pi * (120 - 110 * i / ln) * i / SR) * (1 - i / ln) ** 2 for i in range(ln)]
        env = note_env(ln, release=0.05)
        for i in range(ln):
            if t0 + i < n:
                buf[t0 + i] += k[i] * env[i] * 0.9
        # snare on backbeat
        if bi % 4 == 2:
            sn = noise(int(0.16 * SR), 100 + bi)
            senv = adsr(len(sn), 0.005, 0.05, 0.0, 0.12)
            for i in range(len(sn)):
                if t0 + i < n:
                    buf[t0 + i] += sn[i] * senv[i] * 0.35
        # hats every 8th note
        hh_t0 = t0
        hln = int(0.05 * SR)
        hn = noise(hln, 200 + bi)
        for i in range(hln):
            if hh_t0 + i < n:
                buf[hh_t0 + i] += hn[i] * math.exp(-i / 400) * 0.12
        if bi % 2 == 1 and hh_t0 + int(beat * SR / 2) < n:
            h2 = noise(hln, 300 + bi)
            t2 = hh_t0 + int(beat * SR / 2)
            for i in range(hln):
                if t2 + i < n:
                    buf[t2 + i] += h2[i] * math.exp(-i / 400) * 0.08

    # arpeggiated lead (square-ish) over chords
    for ci, (_, tones) in enumerate(progression):
        for bi in range(16):
            t0 = int((ci * 2 + bi / 2) * beat * SR)
            tone = tones[bi % len(tones)]
            ln = int(beat * SR * 0.45)
            osc = []
            for i in range(ln):
                v = 1.0 if (i * tone * 2 / SR) % 1 < 0.5 else -1.0
                osc.append(v * 0.5 + math.sin(2 * math.pi * tone * 2 * i / SR) * 0.1)
            osc = lowpass(osc, 2600)
            env = note_env(ln, release=0.04)
            for i in range(ln):
                if t0 + i < n:
                    buf[t0 + i] += osc[i] * env[i] * 0.18

    peak = max(map(abs, buf)) or 1
    gain = 0.85 / peak
    pcm = bytearray()
    for x in buf:
        s = clamp((x * gain + 1) * 32767.5, -32768, 32767)
        pcm += struct.pack("<h", s)
    with open(os.path.join(AUDIO, "soundtrack_raw.pcm"), "wb") as f:
        f.write(pcm)
    print(f"[audio] soundtrack PCM written ({dur}s, {bpm} BPM)")


if __name__ == "__main__":
    main()
