//! Structural analysis library for APK, PE and ELF files.
//!
//! Deliberately scoped to *structural* analysis — hashing, container inventory,
//! manifest decoding, import/export tables, DEX class inventory, entropy and
//! string extraction. Instruction-level disassembly and decompilation need a real
//! engine (Ghidra/IDA), which is not available here; see docs/BINARY_ANALYSIS.md.

pub mod apk;
pub mod axml;
pub mod dex;
pub mod elf;
/// Re-exported from `forge-core` so there is exactly one hash implementation.
pub use forge_core::hashes;
pub mod report;
pub mod xmltext;
pub mod pe;
