# Binary analysis — APK and EXE

Last updated 2026-10-30. Everything in this document was produced by running the code in
this repository against the fixtures in `tests/fixtures/`, and by testing the third-party
tool that was originally proposed for the job. Where a capability is missing, the reason is
stated instead of being hidden.

## 1. The question this answers

> "If I give you an APK or an EXE, can you reverse engineer it?"

Reverse engineering has two very different halves:

| Half | What it produces | Available here? |
|---|---|---|
| **Structural analysis** | what the file *is*: container layout, manifest, dependencies, entry points, imports, exports, class inventory, strings, entropy, packer/signing indicators | **yes — implemented in Rust in this repo** |
| **Code recovery** | Java/Kotlin source from DEX, x86/ARM assembly, decompiled pseudo-C, symbolised control flow | **no — needs a JVM toolchain (apktool/jadx) for Android, and Ghidra/IDA for native code, none of which exist in this image** |

So the honest answer is: **structural reverse engineering yes, source-level decompilation no.**

## 2. What the third-party tool actually does (measured, not assumed)

`rea-agents@6.8.0` was installed and driven directly. Results on 2026-10-30:

```
$ rea analyze sample.apk --provider ghidra
error: capability_unavailable  (target_unsupported)
       candidates: ghidra, hopper, ida, kuna — all not_configured

$ rea analyze sample.exe --provider ghidra
error: provider_unavailable
       GHIDRA_INSTALL_DIR is not set

$ rea inspect-artifact sample.apk
ok  format: apk   sha256: 7e1e103d...   provider: rea-artifact-graph
```

`inspect-artifact` (hashing and inventory) works. `analyze` — the part that would actually
reverse the code — requires a commercial or platform-locked engine that is not installed and
cannot be installed on a zero-budget Linux image (IDA/Hopper are paid and/or macOS-only;
Ghidra needs a JVM, and `java` is absent from this image alongside `apktool`, `jadx` and
`dex2jar`).

Measured tool presence in this image:

```
absent : java javap apktool jadx dexdump d8 aapt2 file
present: unzip strings objdump readelf nm python3 gcc
```

Therefore REA is **not** the tool that makes this work here. Forge implements the analysis
itself, and records the REA finding as evidence (`docs/REA_EVIDENCE_INDEX.md`).

## 3. What Forge implements — `crates/bininspect`

Zero dependencies beyond `forge-core` (which is itself std-only). No JVM, no shelling out, no
network. It is both a library and a CLI:

```
cargo run --offline -p bininspect -- <file> [--json] [--strings N]
```

| Module | Responsibility |
|---|---|
| `hashes.rs` | SHA-256, SHA-1, Shannon entropy, ASCII and UTF-16LE string extraction — all hand-written and tested against published vectors |
| `apk.rs` | ZIP container inventory, entry sizes + per-entry entropy, manifest/DEX/lib/signature discovery, risk-indicator assembly |
| `axml.rs` | Android binary XML: string pools (UTF-8 and UTF-16), typed attribute values, element tree → `ManifestSummary` |
| `dex.rs` | DEX 035–039: MUTF-8 string table, type/method ids, `class_data` walk → classes, methods, and reference findings |
| `pe.rs` | PE32/PE32+: headers, sections (+ per-section entropy), imports (names and ordinals), exports, `.NET` CLR/BSJB metadata, `#Strings`/`#US`, version info, overlay, packer indicators |
| `elf.rs` | ELF identification for `.so` files shipped inside APKs: class, machine, type, entry, sections, stripped/not |
| `report.rs` | Format detection and the unified JSON report used by the CLI and the HTTP API |

### APK: what is reported

* package id, version name/code, min and target SDK
* every permission declared (sorted, deduplicated)
* activities, services, receivers, providers with their **exported** flag
* `debuggable`, `allowBackup`, `usesCleartextTraffic`
* DEX inventory: class count, method count, string count, application class list
* code findings from the DEX reference graph: crypto APIs, network APIs, dynamic code loading,
  root/emulator checks, hardcoded cleartext URLs
* native libraries and ABIs, assets, `META-INF` signature files, container entry count and
  total uncompressed size, largest entries with entropy

Real output from `tests/fixtures/apk/sample.apk`:

```
package     : com.example.forgetest
version     : 1.2.3 (code 42)
sdk         : min 24 / target 34
debuggable  : true
dex         : ["classes.dex"] — 3 classes, 8 methods, 18 strings
code findings:
  [cleartext_url] hardcoded cleartext URL: `http://api.forgetest.example/v1/upload`
  [crypto] crypto APIs referenced: javax/crypto, Cipher
  [network] network APIs referenced: HttpURLConnection, loadUrl
  [root_detection] string looks like a root/emulator check: `/system/xbin/su`
risk indicators:
  ! application is marked android:debuggable="true" …
  ! exported component (reachable by other apps): com.example.forgetest.MainActivity
  ! 1 native libraries for ABI(s) arm64-v8a — code analysis needs a native disassembler
```

### EXE: what is reported

* architecture (x86 / x86-64 / ARM64), subsystem, entry RVA, image base
* section table with characteristics and **per-section entropy**
* imports per DLL with function names, exports by name
* `.NET` detection, CLR metadata version, `#Strings` and `#US` user strings
* `VS_VERSIONINFO` key/value pairs
* anomalies: high-entropy sections (> 7.2 bits/byte), W+X sections, UPX-style names, tiny or
  absent import table, overlay appended past the last section

Real output from `tests/fixtures/sample.exe`:

```
arch        : 64-bit x86-64
subsystem   : Windows console
entry point : 0x1000   image base: 0x140000000
section  vaddr  size  flags               entropy
.text    0x1000   512  exec|read|code      0.08
.rdata   0x2000  1024  read|idata          0.56
.data    0x3000   512  read|write|idata    4.00
.evil    0x4000  1024  exec|read|write|code 7.80
imports: KERNEL32.dll  3 fn  CreateFileW, ReadFile, WriteFile
anomalies:
  ! high-entropy section(s) .evil (>7.2 bits/byte): packed or encrypted payload
  ! W+X section `.evil` present
```

## 4. HTTP API

`POST /api/binary/inspect` — wired in `crates/forge-server/src/binary.rs` (it lives in the
server crate, not `forge-core::api`, because `bininspect` depends on `forge-core` and the
reverse direction would be a dependency cycle).

Two request shapes:

```bash
# raw bytes
curl -X POST --data-binary @app.apk \
  'http://127.0.0.1:8080/api/binary/inspect?name=app.apk'

# JSON + base64 (for clients that cannot stream bytes)
curl -X POST -H 'Content-Type: application/json' \
  -d '{"filename":"app.apk","data_base64":"UEsDBBQ…"}' \
  http://127.0.0.1:8080/api/binary/inspect
```

Response: `{"ok":true,"report":{…}}`. Errors: `405` wrong method, `400` empty body or bad
JSON/base64, `413` over 256 MiB. Unknown formats still return `200` with
`"format":"unknown"` — an unidentifiable file is a result, not a server error.

The artifact is analysed **in memory only**: nothing is written to disk, nothing is executed,
and archives are never extracted to the filesystem (no zip-slip surface).

## 5. Web UI

The capabilities page carries a *Binary inspection (APK / EXE)* panel: pick a file, press
Analyse, and the same JSON report is rendered as identity, container, manifest, code findings,
PE sections/imports and anomalies. Verified by `tests/ui-smoke.js`, which feeds real reports
for both fixtures through the real renderer.

## 6. Where analysis stops — and what would extend it

| Missing capability | Why | What would unlock it |
|---|---|---|
| Java/Kotlin source from DEX | no JVM, no apktool/jadx/dex2jar | install `openjdk` + `apktool`/`jadx` (needs a working apt route or a vendored JDK) |
| DEX→smali | not implemented; writing a full smali printer is a project of its own | extend `dex.rs` to decode instructions from `code_item` and print mnemonics |
| x86/ARM disassembly | no disassembler engine; writing a decoder is out of scope | install Ghidra (needs JVM) or drive `objdump -d` when a GNU binutils with the right target is present |
| Control-flow graphs, decompiled pseudo-C | same as above | Ghidra headless (`analyzeHeadless`) |
| APK signature verification (v2/v3) | CMS/PKCS#7 + APK Signing Block parsing not implemented | implement CMS verification or shell out to `apksigner` |
| Native `.so` disassembly inside an APK | `elf.rs` reports headers and sections only | Ghidra/objdump per architecture |

The `objdump`/`readelf` binaries do exist in this image, which means **a future step could
shell out to `objdump -d` for ELF and to a MinGW-targeted objdump for PE** — that is the
cheapest honest path to real instruction listing, and it is deliberately not claimed here
because it has not been implemented or tested yet.

## 7. Verification

```bash
cd NLLM2
cargo test --offline -p bininspect          # 20 tests
cargo test --offline                        # 117 total (20 + 87 lib + 5 e2e + 5 server)
node tests/ui-smoke.js                      # 36 assertions against a live server
```

Fixture generators (kept in-tree so the fixtures are reproducible, not magic binaries):

```bash
python3 tests/fixtures/make_apk.py   # AXML + DEX 035 + ZIP → apk/sample.apk
python3 tests/fixtures/make_pe.py    # PE32+ with imports and a W+X high-entropy section
```
