# Bison source selection — dead ends and decision (2026-10-26)

## Decision

Build **GNU bison 3.0.4** from `tar-mirror/gnu-bison` (GitHub mirror of the
upstream release tarball). Pin: `PIN_bison-rel` in `pins.env`.
Stage: `scripts/build_extras.sh s_bison`.

Release tarballs ship pregenerated `scan-*.c`, `parse-gram.c/.h`,
`configure`, `aclocal.m4` and all `Makefile.in`, so neither flex nor a
gnulib/gettext bootstrap is required. One source patch is needed:

- `lib/fseterr.c` — modern glibc (>=2.32) no longer exposes `_IO_ftrylockfile`
  and `__GNU_LIBRARY__` is no longer `1`, which trips the bundled gnulib
  portability `#error`. The stage adds a `__GLIBC__ && __GLIBC_MINOR__`
  branch to the existing `_IO_ERR_SEEN` path (internal libio layout is
  unchanged). Applied idempotently via a guarded `sed`.

## Rejected alternatives

### 1. `gnu-mirror-unofficial/bison` VCS snapshot (tip 2022-01-23, 1183d5e)

Raw VCS tree with empty `gnulib` and `submodules/autoconf` submodules
pointing at unreachable `git://git.savannah.gnu.org` / `git://git.sv.gnu.org`
URLs (egress allowlist: github.com, pypi.org, registry.npmjs.org only).
A full bootstrap was driven manually and got as far as compiling bison
sources, but accumulated unresolved structural problems:

- No `autopoint` on the system; gettext is not installed. A no-op autopoint
  shim plus `gettext-runtime/m4` from `deepin-community/gettext` was needed
  to satisfy `AM_GNU_GETTEXT` / `AM_GNU_GETTEXT_VERSION`.
- `AC_PROG_GNU_M4` is defined only in gettext-tools' m4 archive, absent from
  every reachable packaging; required a hand-written stub macro.
- Version derives from `build-aux/git-version-gen`; a shallow clone yields
  version `UNKNOWN`, rejected by Gnits strictness — needs a fabricated
  `.tarball-version`.
- `po/` and `runtime-po/` gettext scaffolding (`Makefile.in.in`, `Makevars`,
  `LINGUAS`, `Rules-quot`, ...) normally installed by autopoint had to be
  assembled by hand; the gettext-0.23 template's `@top_builddir@` is not
  substituted by this bison's configure, requiring gnulib's variant with a
  literal `top_builddir = ..`.
- gnulib master (2026) is API-skewed against the 2022 bison snapshot:
  `create_pipe_bidi` gained a `dll_dirs` argument; `TEXTSTYLE_H` substitutes
  to a bare `textstyle.h` while the generation rule emits `lib/textstyle.h`
  (prefix variable only in scope inside `gl_INIT`). Checking out gnulib
  commit `71f29a9f4` (2022-01-19) removed the API skew but not the
  TEXTSTYLE_H substitution issue.
- `data/m4sugar/*.m4` and `build-aux/move-if-change` are symlinks into the
  unreachable autoconf submodule (3 dangling links); had to be repointed at
  the locally built autoconf tree and the gnulib checkout.
- Hard blocker: VCS trees generate `scan-code.c`, `scan-gram.c`,
  `scan-skel.c` from `.l` files with **flex**, which is neither installed nor
  downloadable inside the egress allowlist (no Debian mirror access; no
  credible precompiled FOSS binary on GitHub).

### 2. `deepin-community/bison` — Debian-packaged, stripped tree

Missing generated `Makefile.in` and `doc/bison.texi`; not buildable as-is.

### 3. `openela-main/bison` — RPM spec repository

Only `SOURCES/`/`SPECS/` packaging metadata, no upstream source tree.

## Consequence

bison 3.0.4 is older than the `>= 3.6` comment in the old build_stack
header. libxkbcommon release tarballs ship a pregenerated `xkb-parser.c`,
and meson only invokes bison when parser regeneration is explicitly
enabled (it is not in this build), so 3.0.4 is sufficient for the Blender
dependency chain. If a future component genuinely requires bison >= 3.6,
the only free path inside the allowlist is packaging a flex release first
(chicken-and-egg: flex itself needs a bison/flex to build from VCS; a
flex release-tarball mirror on GitHub would have to be located).
