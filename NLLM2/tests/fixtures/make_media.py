#!/usr/bin/env python3
"""Provenance for `tests/fixtures/sample.mp4`.

This fixture is **our own synthetic clip**, not material from any product or platform.
It exists so `media::inspect` and the frame-extraction research below are tested against
real encoder output instead of a hand-built box tree (which is also tested, in
`media.rs::tests::sample_mp4`).

Generated with a static FFmpeg 7.0.2 build (installed from the `imageio-ffmpeg` wheel;
`ffprobe` is not part of it). The exact command:

    ffmpeg -y \
      -f lavfi -i "color=c=red:s=320x240:r=10:d=2" \
      -f lavfi -i "color=c=green:s=320x240:r=10:d=2" \
      -f lavfi -i "color=c=blue:s=320x240:r=10:d=2" \
      -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]" \
      -map "[v]" -c:v libx264 -pix_fmt yuv420p -g 10 -t 6 -r 10 \
      tests/fixtures/sample.mp4

What that produces, measured by our own parser (`media::tests::a_real_encoder_output_is_read_correctly`)
and by FFmpeg itself:

    container mp4, brand isom
    duration  6.0 s (mvhd timescale 10240)
    video     avc1, 320x240, yuv420p, 10 fps, 60 frames

Regenerating it is optional; the checked-in file is what the tests read. If you do not have
FFmpeg, delete nothing — the test skips itself only when the fixture is absent.

Frame extraction used for research (see docs/reference/VIDEO_RESEARCH.md). FFmpeg 7.x needs
`-fps_mode vfr` for scene dumps; `-vsync` is deprecated and fails:

    # one contact sheet: 3 frames across, sampled at 0.5 fps
    ffmpeg -i sample.mp4 -vf "fps=0.5,scale=160:-1,tile=3x1" -frames:v 1 /tmp/sheet.png

    # scene changes, one PNG per detected shot
    ffmpeg -i sample.mp4 -vf "select='gt(scene,0.3)'" -fps_mode vfr /tmp/scenes/s-%03d.png

The clip has exactly two scene cuts (red→green at 2 s, green→blue at 4 s), so the expected
result is exactly two PNGs. That is the assertion the research log records.

There is no FFmpeg dependency in the product: `crates/forge-core` parses the container by
hand and never shells out to FFmpeg.
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "sample.mp4")


def main() -> int:
    ff = os.environ.get("FFMPEG", "ffmpeg")
    if not os.path.exists(OUT):
        print(f"fixture missing: {OUT}", file=sys.stderr)
        return 1
    try:
        subprocess.run([ff, "-version"], check=True, capture_output=True)
    except (OSError, subprocess.CalledProcessError):
        print("ffmpeg not available; the checked-in fixture is already present, nothing to do")
        return 0
    print(f"{OUT}: {os.path.getsize(OUT)} bytes")
    print("regenerate with the command in the module docstring")
    return 0


if __name__ == "__main__":
    sys.exit(main())
