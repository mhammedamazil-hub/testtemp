#!/usr/bin/env python3
"""Generate a structurally valid APK fixture for bininspect tests.

Not a real app: a hand-built ZIP containing a compiled (binary) AndroidManifest.xml,
a Dalvik classes.dex with a real header/string/type/method/class-def layout, a
signature placeholder and a native-lib placeholder. Enough to exercise every parser
path without shipping someone else's binary in the repo.

Run: python3 tests/fixtures/make_apk.py
"""
import os
import struct
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "apk")
STAMP = (2026, 1, 1, 0, 0, 0)


# ----------------------------------------------------------------- AXML writer
def build_string_pool(strings):
    """UTF-8 flavoured ResStringPool."""
    offsets, data = [], b""
    for s in strings:
        offsets.append(len(data))
        raw = s.encode("utf-8")
        u16 = len(s)  # BMP-only strings here
        assert u16 < 0x80 and len(raw) < 0x80
        data += bytes([u16, len(raw)]) + raw + b"\x00"
    while len(data) % 4:
        data += b"\x00"
    header_size = 28
    size = header_size + len(offsets) * 4 + len(data)
    out = struct.pack("<HHIIIIII", 0x0001, header_size, size, len(strings), 0,
                      0x00000100, header_size + len(offsets) * 4, 0)
    out += b"".join(struct.pack("<I", o) for o in offsets)
    out += data
    return out


class Pool:
    def __init__(self, strings):
        self.strings = list(strings)

    def idx(self, s):
        if s not in self.strings:
            self.strings.append(s)
        return self.strings.index(s)


def attr(pool, name, value, ns=None, type_id=0x03):
    ns_i = 0xFFFFFFFF if ns is None else pool.idx(ns)
    name_i = pool.idx(name)
    if type_id == 0x03:
        raw_i = pool.idx(value)
        data = raw_i
    elif type_id == 0x12:
        raw_i = 0xFFFFFFFF
        data = 0xFFFFFFFF if value in (True, "true") else 0
    else:
        raw_i = 0xFFFFFFFF
        data = int(value)
    return struct.pack("<IIIHBBI", ns_i, name_i, raw_i, 8, 0, type_id, data)


def start_element(pool, name, attrs):
    name_i = pool.idx(name)
    body = struct.pack("<II", 0xFFFFFFFF, name_i)
    body += struct.pack("<HHHHHH", 20, 20, len(attrs), 0, 0, 0)
    body += b"".join(attrs)
    header = struct.pack("<HHIII", 0x0102, 16, 16 + len(body), 1, 0xFFFFFFFF)
    return header + body


def end_element(pool, name):
    body = struct.pack("<II", 0xFFFFFFFF, pool.idx(name))
    return struct.pack("<HHIII", 0x0103, 16, 16 + len(body), 1, 0xFFFFFFFF) + body


ANDROID_NS = "http://schemas.android.com/apk/res/android"


def build_manifest():
    pool = Pool([])
    chunks = []

    # <manifest package=... versionCode=... versionName=...>
    chunks.append(start_element(pool, "manifest", [
        attr(pool, "versionCode", 42, ANDROID_NS, 0x10),
        attr(pool, "versionName", "1.2.3", ANDROID_NS),
        attr(pool, "package", "com.example.forgetest"),
    ]))
    # <uses-sdk minSdkVersion=24 targetSdkVersion=34/>
    chunks.append(start_element(pool, "uses-sdk", [
        attr(pool, "minSdkVersion", 24, ANDROID_NS, 0x10),
        attr(pool, "targetSdkVersion", 34, ANDROID_NS, 0x10),
    ]))
    chunks.append(end_element(pool, "uses-sdk"))
    # <uses-permission android:name="android.permission.INTERNET"/>
    for perm in ("android.permission.INTERNET", "android.permission.CAMERA"):
        chunks.append(start_element(pool, "uses-permission", [
            attr(pool, "name", perm, ANDROID_NS),
        ]))
        chunks.append(end_element(pool, "uses-permission"))
    # <application debuggable=true allowBackup=true label=ForgeTest>
    chunks.append(start_element(pool, "application", [
        attr(pool, "debuggable", True, ANDROID_NS, 0x12),
        attr(pool, "allowBackup", True, ANDROID_NS, 0x12),
        attr(pool, "label", "ForgeTest", ANDROID_NS),
    ]))
    #   <activity android:name=".MainActivity" android:exported="true">
    chunks.append(start_element(pool, "activity", [
        attr(pool, "name", "com.example.forgetest.MainActivity", ANDROID_NS),
        attr(pool, "exported", True, ANDROID_NS, 0x12),
    ]))
    chunks.append(start_element(pool, "intent-filter", []))
    chunks.append(start_element(pool, "action", [
        attr(pool, "name", "android.intent.action.MAIN", ANDROID_NS),
    ]))
    chunks.append(end_element(pool, "action"))
    chunks.append(end_element(pool, "intent-filter"))
    chunks.append(end_element(pool, "activity"))
    #   <service android:name=".SyncService" android:exported="false"/>
    chunks.append(start_element(pool, "service", [
        attr(pool, "name", "com.example.forgetest.SyncService", ANDROID_NS),
        attr(pool, "exported", False, ANDROID_NS, 0x12),
    ]))
    chunks.append(end_element(pool, "service"))
    chunks.append(end_element(pool, "application"))
    chunks.append(end_element(pool, "manifest"))

    body = b"".join(chunks)
    pool_bytes = build_string_pool(pool.strings)
    total = 8 + len(pool_bytes) + len(body)
    return struct.pack("<HHI", 0x0003, 8, total) + pool_bytes + body, pool.strings


# ------------------------------------------------------------------ DEX writer
def uleb(n):
    out = b""
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out += bytes([b | 0x80])
        else:
            out += bytes([b])
            return out


def build_dex():
    strings = [
        "Lcom/example/forgetest/MainActivity;",
        "Lcom/example/forgetest/Crypto;",
        "Lcom/example/forgetest/SyncService;",
        "Landroid/app/Activity;",
        "Ljavax/crypto/Cipher;",
        "Ljava/net/HttpURLConnection;",
        "Ljava/lang/String;",
        "V",
        "<init>",
        "onCreate",
        "encrypt",
        "getInstance",
        "doFinal",
        "connect",
        "http://api.forgetest.example/v1/upload",
        "/system/xbin/su",
        "loadUrl",
        "AES/GCM/NoPadding",
    ]
    s_index = {s: i for i, s in enumerate(strings)}

    sdata, soffs = b"", []
    for s in strings:
        soffs.append(len(sdata))
        raw = s.encode("utf-8")
        sdata += uleb(len(s)) + raw + b"\x00"
    while len(sdata) % 4:
        sdata += b"\x00"

    types = list(strings)  # one type per string, plenty for a fixture
    n, m = len(strings), 8
    class_defs = 3  # MainActivity, Crypto, SyncService

    string_ids_off = 0x70
    type_ids_off = string_ids_off + 4 * n
    proto_ids_off = type_ids_off + 4 * len(types)
    method_ids_off = proto_ids_off + 12 * m
    class_defs_off = method_ids_off + 8 * m
    class_data_off = class_defs_off + 32 * class_defs
    data_off = class_data_off + 200  # leaves room for class_data blobs

    def method(class_name, method_name):
        c = s_index[class_name]
        return struct.pack("<HHI", c, 0, s_index[method_name])

    methods = [
        method("Lcom/example/forgetest/MainActivity;", "<init>"),
        method("Lcom/example/forgetest/MainActivity;", "onCreate"),
        method("Lcom/example/forgetest/MainActivity;", "loadUrl"),
        method("Lcom/example/forgetest/Crypto;", "encrypt"),
        method("Lcom/example/forgetest/Crypto;", "<init>"),
        method("Ljavax/crypto/Cipher;", "doFinal"),
        method("Ljavax/crypto/Cipher;", "getInstance"),
        method("Ljava/net/HttpURLConnection;", "connect"),
    ]

    # --- class_data blobs (per class: static fields, instance fields, direct, virtual)
    blobs = b""
    cdef_entries = []
    for ci, (cls, mids) in enumerate([
        ("Lcom/example/forgetest/MainActivity;", [0, 1, 2]),   # indices into methods
        ("Lcom/example/forgetest/Crypto;", [3, 4]),
        ("Ljavax/crypto/Cipher;", [5, 6]),
    ]):
        off = class_data_off + len(blobs)
        blob = uleb(0) + uleb(0) + uleb(len(mids)) + uleb(0)
        prev = 0
        for i, mi in enumerate(mids):
            blob += uleb(mi - prev if i else mi) + uleb(0x1) + uleb(0)
            prev = mi
        blobs += blob
        while len(blobs) % 4:
            blobs += b"\x00"
        cdef_entries.append((s_index[cls], off))

    hdr = bytearray(0x70)
    hdr[0:8] = b"dex\n035\x00"

    def put(off, fmt, *v):
        struct.pack_into(fmt, hdr, off, *v)

    put(36, "<I", 0x70)
    put(40, "<I", 0x12345678)
    # DEX header field offsets (decimal):
    #  56/60 string_ids size/off   64/68 type_ids   72/76 proto_ids
    #  80/84 field_ids (unused)    88/92 method_ids  96/100 class_defs
    # 104/108 data size/off
    put(56, "<I", n);            put(60, "<I", string_ids_off)
    put(64, "<I", len(types));   put(68, "<I", type_ids_off)
    put(72, "<I", m);            put(76, "<I", proto_ids_off)
    put(80, "<I", 0);            put(84, "<I", 0)
    put(88, "<I", m);            put(92, "<I", method_ids_off)
    put(96, "<I", class_defs);   put(100, "<I", class_defs_off)
    put(104, "<I", 0);           put(108, "<I", data_off)

    sids = b"".join(struct.pack("<I", data_off + o) for o in soffs)
    tids = b"".join(struct.pack("<I", i) for i in range(len(types)))
    pids = b"".join(struct.pack("<III", 0, i, 0) for i in range(m))
    mids = b"".join(methods)
    cdefs = b"".join(struct.pack("<IIIIIIII", cls_i, 0x1, 0xFFFFFFFF, 0, 0xFFFFFFFF, 0, off, 0)
                     for cls_i, off in cdef_entries)

    blob = bytearray(bytes(hdr) + sids + tids + pids + mids + cdefs)
    blob += b"\x00" * (class_data_off - len(blob))
    blob += blobs
    blob += b"\x00" * (data_off - len(blob)) if len(blob) < data_off else b""
    blob += sdata
    struct.pack_into("<I", blob, 32, len(blob))
    struct.pack_into("<I", blob, 104, len(blob) - data_off)
    return bytes(blob), strings


def main():
    os.makedirs(OUT, exist_ok=True)
    manifest, mstrings = build_manifest()
    dex, dstrings = build_dex()
    open(os.path.join(OUT, "AndroidManifest.xml"), "wb").write(manifest)
    open(os.path.join(OUT, "classes.dex"), "wb").write(dex)

    apk_path = os.path.join(OUT, "sample.apk")
    with zipfile.ZipFile(apk_path, "w", zipfile.ZIP_DEFLATED) as z:
        for name, blob in (
            ("AndroidManifest.xml", manifest),
            ("classes.dex", dex),
            ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\r\nCreated-By: Forge fixture\r\n"),
            ("META-INF/CERT.RSA", b"\x30\x82\x01\x00" + b"CN=Forge Test Fixture,O=Forge\x00" + b"\x00" * 64),
            ("res/values/strings.xml", b'<resources><string name="app_name">ForgeTest</string></resources>'),
            ("assets/config.json", b'{"endpoint":"http://api.forgetest.example/v1/upload"}'),
            ("lib/arm64-v8a/libnative.so", b"\x7fELF\x02\x01\x01\x00" + b"\x00" * 56),
        ):
            info = zipfile.ZipInfo(name, date_time=STAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, blob)

    print(f"wrote {apk_path} ({os.path.getsize(apk_path)} bytes)")
    print(f"  manifest: {len(manifest)} bytes, {len(mstrings)} pool strings")
    print(f"  dex:      {len(dex)} bytes, {len(dstrings)} strings, {len(dstrings)} types, 8 methods")


if __name__ == "__main__":
    main()
