//! bininspect — structural analysis for APK / EXE / ELF files, with no third-party
//! code and no network.
//!
//! Answers the first questions an analyst asks: what is this, what does it import,
//! is it packed, is it managed code, what components does it expose, what strings
//! and URLs does it carry.
//!
//! Usage:
//!     bininspect <file> [--json] [--strings N]

use bininspect::report::analyze;
use forge_core::json;
use forge_core::json::Value;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut path: Option<String> = None;
    let mut as_json = false;
    let mut string_limit = 40usize;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--json" => as_json = true,
            "--strings" => {
                string_limit = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(40)
            }
            "-h" | "--help" => {
                println!("usage: bininspect <file> [--json] [--strings N]");
                println!("  analyses APK (ZIP+AXML+DEX), PE (EXE/DLL) and ELF files offline");
                return;
            }
            other => path = Some(other.to_string()),
        }
    }
    let Some(path) = path else {
        eprintln!("usage: bininspect <file> [--json] [--strings N]");
        std::process::exit(2);
    };

    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot read {}: {}", path, e);
            std::process::exit(1);
        }
    };

    let report = analyze(&path, &data, string_limit);
    if as_json {
        println!("{}", json::to_pretty(&report));
    } else {
        print_human(&path, &data, &report, string_limit);
    }
}

fn section(title: &str) {
    println!("\n== {} ==", title);
}

fn print_human(path: &str, data: &[u8], report: &Value, limit: usize) {
    println!("file      : {}", path);
    println!("size      : {} bytes", data.len());
    println!("sha256    : {}", report.str_of("sha256").unwrap_or(""));
    println!("sha1      : {}", report.str_of("sha1").unwrap_or(""));
    println!("entropy   : {:.2} bits/byte", report.get("entropy").and_then(|e| e.as_f64()).unwrap_or(0.0));
    println!("format    : {}", report.str_of("format").unwrap_or("unknown"));

    if let Some(err) = report.str_of("error") {
        println!("\nERROR: {}", err);
        return;
    }

    if let Some(p) = report.get("pe") {
        section("PE (Windows executable)");
        println!("arch        : {}-bit {}", p.get("bits").and_then(|b| b.as_f64()).unwrap_or(0.0) as u64, p.str_of("machine").unwrap_or(""));
        println!("subsystem   : {}", p.str_of("subsystem").unwrap_or(""));
        println!("entry point : {}   image base: {}", p.str_of("entry_point_rva").unwrap_or(""), p.str_of("image_base").unwrap_or(""));
        println!("managed     : {}{}", p.get("dotnet").and_then(|d| d.as_bool()).unwrap_or(false),
            if p.str_of("dotnet_version").unwrap_or("").is_empty() { String::new() } else { format!(" ({})", p.str_of("dotnet_version").unwrap_or("")) });
        if let Some(secs) = p.get("sections").and_then(|s| s.as_arr()) {
            println!("\n{:<10} {:>10} {:>8}  {:<16} entropy", "section", "vaddr", "size", "flags");
            for s in secs {
                println!(
                    "{:<10} {:>10} {:>8}  {:<16} {:.2}",
                    s.str_of("name").unwrap_or(""),
                    s.str_of("vaddr").unwrap_or(""),
                    s.get("raw_size").and_then(|x| x.as_f64()).unwrap_or(0.0) as u64,
                    s.str_of("flags").unwrap_or(""),
                    s.get("entropy").and_then(|x| x.as_f64()).unwrap_or(0.0)
                );
            }
        }
        if let Some(imps) = p.get("imports").and_then(|i| i.as_arr()) {
            if !imps.is_empty() {
                section("imports");
                for i in imps {
                    let fns = i.get("functions").and_then(|f| f.as_arr()).cloned().unwrap_or_default();
                    let names: Vec<String> = fns
                        .iter()
                        .take(12)
                        .filter_map(|f| f.as_str().map(|s| s.to_string()))
                        .collect();
                    println!("{:<28} {:>3} fn  {}", i.str_of("dll").unwrap_or(""), fns.len(), names.join(", "));
                }
            }
        }
        if let Some(exps) = p.get("exports").and_then(|e| e.as_arr()) {
            if !exps.is_empty() {
                println!("\nexports: {}", exps.iter().take(20).filter_map(|e| e.as_str()).collect::<Vec<_>>().join(", "));
            }
        }
        if let Some(vi) = p.get("version_info").and_then(|v| v.as_arr()) {
            if !vi.is_empty() {
                section("version info");
                for kv in vi {
                    println!("{:<18} {}", kv.str_of("key").unwrap_or(""), kv.str_of("value").unwrap_or(""));
                }
            }
        }
        if let Some(ind) = p.get("indicators").and_then(|i| i.as_arr()) {
            if !ind.is_empty() {
                section("anomalies / packer signals");
                for i in ind {
                    println!("  ! {}", i.as_str().unwrap_or(""));
                }
            }
        }
        for (label, key) in [("ASCII strings (first {})", "ascii_strings"), ("wide strings (first {})", "wide_strings")] {
            if let Some(list) = p.get(key).and_then(|s| s.as_arr()).filter(|s| !s.is_empty()) {
                section(&label.replace("{}", &limit.min(list.len()).to_string()));
                for x in list.iter().take(limit) {
                    println!("  {}", x.as_str().unwrap_or(""));
                }
            }
        }
    }

    if let Some(a) = report.get("apk") {
        section("APK");
        println!("package     : {}", a.str_of("package").unwrap_or(""));
        println!("version     : {} (code {})", a.str_of("version_name").unwrap_or(""), a.str_of("version_code").unwrap_or(""));
        println!("sdk         : min {} / target {}", a.str_of("min_sdk").unwrap_or("?"), a.str_of("target_sdk").unwrap_or("?"));
        println!("debuggable  : {}", a.get("debuggable").and_then(|d| d.as_bool()).unwrap_or(false));
        println!("allowBackup : {}", a.get("allow_backup").and_then(|d| d.as_bool()).unwrap_or(false));
        println!("cleartext   : {}", a.get("cleartext_traffic").and_then(|d| d.as_bool()).unwrap_or(false));
        println!("dex         : {:?} — {} classes, {} methods, {} strings",
            a.get("dex_files").and_then(|d| d.as_arr()).map(|v| v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect::<Vec<_>>()).unwrap_or_default(),
            a.get("classes").and_then(|c| c.as_f64()).unwrap_or(0.0) as u64,
            a.get("methods").and_then(|c| c.as_f64()).unwrap_or(0.0) as u64,
            a.get("strings").and_then(|c| c.as_f64()).unwrap_or(0.0) as u64);
        println!("containers  : {} entries, {} bytes uncompressed",
            a.get("entries").and_then(|c| c.as_f64()).unwrap_or(0.0) as u64,
            a.get("uncompressed_bytes").and_then(|c| c.as_f64()).unwrap_or(0.0) as u64);

        if let Some(p) = a.get("permissions").and_then(|p| p.as_arr()) {
            section("permissions");
            for x in p {
                println!("  {}", x.as_str().unwrap_or(""));
            }
        }
        for group in ["activities", "services", "receivers", "providers"] {
            if let Some(list) = a.get(group).and_then(|x| x.as_arr()) {
                if list.is_empty() {
                    continue;
                }
                section(group);
                for c in list {
                    println!(
                        "  {:<55} {}",
                        c.str_of("name").unwrap_or(""),
                        if c.get("exported").and_then(|e| e.as_bool()).unwrap_or(false) { "EXPORTED" } else { "" }
                    );
                }
            }
        }
        if let Some(n) = a.get("native_libs").and_then(|x| x.as_arr()) {
            if !n.is_empty() {
                section("native libraries");
                for x in n {
                    println!("  {}", x.as_str().unwrap_or(""));
                }
            }
        }
        if let Some(f) = a.get("code_findings").and_then(|x| x.as_arr()) {
            if !f.is_empty() {
                section("code findings (from the DEX reference graph)");
                for x in f.iter().take(20) {
                    println!("  [{}] {}", x.str_of("category").unwrap_or(""), x.str_of("detail").unwrap_or(""));
                }
            }
        }
        if let Some(u) = a.get("embedded_urls").and_then(|x| x.as_arr()) {
            if !u.is_empty() {
                section("embedded cleartext URLs");
                for x in u.iter().take(20) {
                    println!("  {}", x.str_of("detail").unwrap_or(""));
                }
            }
        }
        if let Some(i) = a.get("indicators").and_then(|x| x.as_arr()) {
            if !i.is_empty() {
                section("risk indicators");
                for x in i {
                    println!("  ! {}", x.as_str().unwrap_or(""));
                }
            }
        }
    }

    if let Some(m) = report.get("manifest") {
        let kind = if report.str_of("format") == Some("android_binary_xml") { "compiled" } else { "textual" };
        section(&format!("Android manifest ({})", kind));
        println!("package   : {}", m.str_of("package").unwrap_or(""));
        println!("version   : {} (code {})", m.str_of("version_name").unwrap_or(""), m.str_of("version_code").unwrap_or(""));
        println!("sdk       : min {} / target {}", m.str_of("min_sdk").unwrap_or("?"), m.str_of("target_sdk").unwrap_or("?"));
        println!("debuggable: {}", m.get("debuggable").and_then(|d| d.as_bool()).unwrap_or(false));
        println!("launcher  : {}", m.str_of("launcher_activity").unwrap_or("(none)"));
        if let Some(p) = m.get("permissions").and_then(|p| p.as_arr()).filter(|p| !p.is_empty()) {
            println!("permissions:");
            for x in p { println!("  {}", x.as_str().unwrap_or("")); }
        }
        for group in ["activities", "services", "receivers", "providers"] {
            if let Some(list) = m.get(group).and_then(|x| x.as_arr()).filter(|l| !l.is_empty()) {
                section(group);
                for c in list {
                    println!("  {:<52} {}", c.str_of("name").unwrap_or(""),
                        if c.get("exported").and_then(|e| e.as_bool()).unwrap_or(false) { "EXPORTED" } else { "" });
                }
            }
        }
    }

    if let Some(d) = report.get("dex") {
        section("DEX (Dalvik bytecode, standalone)");
        println!("version   : {}", d.str_of("version").unwrap_or(""));
        println!("declared  : {} bytes (header field)", d.get("declared_size").and_then(|x| x.as_f64()).unwrap_or(0.0) as u64);
        println!("inventory : {} classes, {} methods, {} strings, {} types",
            d.get("classes").and_then(|x| x.as_f64()).unwrap_or(0.0) as u64,
            d.get("methods").and_then(|x| x.as_f64()).unwrap_or(0.0) as u64,
            d.get("strings").and_then(|x| x.as_f64()).unwrap_or(0.0) as u64,
            d.get("types").and_then(|x| x.as_f64()).unwrap_or(0.0) as u64);
        if let Some(f) = d.get("code_findings").and_then(|x| x.as_arr()).filter(|f| !f.is_empty()) {
            section("code findings");
            for x in f.iter().take(20) {
                println!("  [{}] {}", x.str_of("category").unwrap_or(""), x.str_of("detail").unwrap_or(""));
            }
        }
        if let Some(c) = d.get("class_names").and_then(|x| x.as_arr()).filter(|c| !c.is_empty()) {
            section(&format!("application classes ({})", c.len()));
            for x in c.iter().take(20) {
                println!("  {}", x.as_str().unwrap_or(""));
            }
        }
    }

    if let Some(l) = report.get("lfs") {
        section("Git LFS pointer");
        println!("oid   : {}", l.str_of("oid").unwrap_or(""));
        println!("size  : {} bytes (the real binary is not here)", l.get("declared_size").and_then(|x| x.as_f64()).unwrap_or(0.0) as u64);
    }

    if let Some(e) = report.get("elf") {
        section("ELF");
        println!("type    : {}", e.str_of("type").unwrap_or(""));
        println!("machine : {}-bit {}", e.get("bits").and_then(|b| b.as_f64()).unwrap_or(0.0) as u64, e.str_of("machine").unwrap_or(""));
        println!("entry   : {}", e.str_of("entry").unwrap_or(""));
        println!("stripped: {}", e.get("stripped").and_then(|s| s.as_bool()).unwrap_or(false));
        if let Some(s) = e.get("sections").and_then(|s| s.as_arr()) {
            println!("sections: {}", s.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "));
        }
    }
}
