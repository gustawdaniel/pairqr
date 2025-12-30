# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Build debug version
cargo build

# Build release version (optimized with LTO and stripped)
cargo build --release

# Install locally
cargo install --path .

# Run directly
cargo run
```

## Cross-Compilation

The project uses `cross` for cross-compilation to various Linux architectures. The release workflow builds for:
- macOS: Universal binary (x86_64 + aarch64)
- Linux: x86_64, i686, aarch64, ARM variants, MIPS, PowerPC, RISC-V, s390x, SPARC64, LoongArch64 (all musl-based static binaries where possible)
- Windows: x64, x86

To cross-compile locally:
```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target <target-triple>
```

## Architecture Overview

This is a single-file Rust CLI application (`src/main.rs`) that enables wireless ADB pairing by generating a QR code.

**Core Flow:**
1. Generates random credentials (service name + password) in Android Studio format
2. Displays a QR code in the terminal using half-block Unicode characters
3. Uses mDNS (`mdns-sd` crate) to discover when the Android device broadcasts a pairing service after scanning the QR
4. Calls `adb pair` with the generated credentials when a device is found

**Key Constants:**
- `SERVICE_TYPE`: mDNS service type for ADB pairing (`_adb-tls-pairing._tcp.local.`)
- QR format: `WIFI:T:ADB;S:<name>;P:<password>;;`

**Dependencies:**
- `mdns-sd`: mDNS service discovery
- `qrcode`: QR code generation
- `tokio`: Async runtime for mDNS event handling
- `ctrlc`/`flume`: Signal handling and channel communication

**External Requirement:** ADB must be installed and in PATH.
