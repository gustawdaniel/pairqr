# TODO: Package Manager Distribution

This document tracks package managers where pairqr should be published. Once a package is available, installation instructions will be added to README.md.

---

## macOS

### Homebrew (Priority: High)
- **Status:** Done
- **Effort:** Medium
- **Description:** The most popular package manager for macOS, also available for Linux
- **Approach:** Create a Homebrew tap repository (`homebrew-pairqr`) with a formula that downloads pre-built binaries from GitHub Releases
- **User install:** `brew install richard-fairthorne/tap/pairqr`
- **Resources:**
  - [How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)
  - [Publishing a Rust CLI to Homebrew](https://kawamurakazushi.com/20200217-publishing-a-rust-cli-to-homebrew/)

### MacPorts (Priority: Low)
- **Status:** Not started
- **Effort:** Medium
- **Description:** Alternative to Homebrew, popular in enterprise environments
- **Approach:** Submit a Portfile to the MacPorts repository
- **User install:** `sudo port install pairqr`

---

## Linux

### crates.io / cargo install (Priority: High)
- **Status:** Done
- **Effort:** Low
- **Description:** Rust's official package registry - targets Rust developers
- **Approach:** `cargo publish` to crates.io
- **User install:** `cargo install pairqr`
- **Note:** Requires users to have Rust toolchain installed
- **Resources:**
  - [Publishing on crates.io](https://doc.rust-lang.org/cargo/reference/publishing.html)

### AUR - Arch User Repository (Priority: Medium)
- **Status:** Ready (needs AUR account to push)
- **Effort:** Medium
- **Description:** Community-driven repository for Arch Linux and derivatives (Manjaro, EndeavourOS)
- **Approach:** Create PKGBUILD that downloads from GitHub Releases or builds from source
- **User install:** `yay -S pairqr` or `paru -S pairqr`
- **Resources:**
  - [Rust package guidelines - ArchWiki](https://wiki.archlinux.org/title/Rust_package_guidelines)
  - [cargo-aur](https://crates.io/crates/cargo-aur) helper tool

### Debian/Ubuntu (Priority: Medium)
- **Status:** Not started
- **Effort:** High
- **Description:** .deb packages for Debian-based distributions
- **Approach:** Use `cargo-deb` to generate .deb packages, host in a PPA or attach to GitHub Releases
- **User install:** `sudo apt install pairqr` (if in PPA)
- **Resources:**
  - [cargo-deb](https://crates.io/crates/cargo-deb)

### Fedora/RHEL (Priority: Low)
- **Status:** Not started
- **Effort:** High
- **Description:** .rpm packages for Red Hat-based distributions
- **Approach:** Use `cargo-rpm` or create spec file manually
- **User install:** `sudo dnf install pairqr`

### Snap (Priority: Low)
- **Status:** Not started
- **Effort:** Medium
- **Description:** Universal Linux packages by Canonical
- **Approach:** Create snapcraft.yaml
- **User install:** `sudo snap install pairqr`
- **Note:** May have sandboxing issues with ADB access

### Flatpak (Priority: Low)
- **Status:** Not started
- **Effort:** High
- **Description:** Universal Linux packages with sandboxing
- **Approach:** Create Flatpak manifest with flatpak-cargo-generator
- **User install:** `flatpak install pairqr`
- **Note:** Sandboxing will likely prevent ADB/network access - may not be suitable
- **Resources:**
  - [Publishing your Rust app as a flatpak](https://develop.kde.org/docs/getting-started/rust/rust-flatpak/)

### Nix (Priority: Low)
- **Status:** Not started
- **Effort:** Medium
- **Description:** Declarative package manager, works on any Linux and macOS
- **Approach:** Submit package to nixpkgs
- **User install:** `nix-env -iA nixpkgs.pairqr`

---

## Windows

### Scoop (Priority: High)
- **Status:** Done
- **Effort:** Low
- **Description:** Command-line installer for Windows, focused on CLI tools. No admin required.
- **Approach:** Create a manifest JSON and submit to scoop-extras bucket or create own bucket
- **User install:** `scoop install pairqr`
- **Resources:**
  - [Scoop Wiki - Creating Manifests](https://github.com/ScoopInstaller/Scoop/wiki/Creating-an-app-manifest)

### winget (Priority: Medium)
- **Status:** PR submitted (https://github.com/microsoft/winget-pkgs/pull/326962)
- **Effort:** Medium
- **Description:** Official Windows Package Manager, ships with Windows 11
- **Approach:** Submit manifest to microsoft/winget-pkgs repository
- **User install:** `winget install pairqr`
- **Resources:**
  - [Submit packages to Windows Package Manager](https://learn.microsoft.com/en-us/windows/package-manager/package/repository)

### Chocolatey (Priority: Medium)
- **Status:** Ready (needs Chocolatey account to push)
- **Effort:** Medium
- **Description:** Largest Windows package repository, popular in enterprise/DevOps
- **Approach:** Create .nuspec package and submit to community repository
- **User install:** `choco install pairqr`
- **Resources:**
  - [Create Chocolatey Packages](https://docs.chocolatey.org/en-us/create/create-packages)

---

## Cross-Platform

### cargo-binstall (Priority: Medium)
- **Status:** Not started
- **Effort:** Low
- **Description:** Downloads pre-built binaries instead of compiling from source
- **Approach:** Add metadata to Cargo.toml pointing to GitHub Releases
- **User install:** `cargo binstall pairqr`
- **Resources:**
  - [cargo-binstall](https://github.com/cargo-bins/cargo-binstall)

---

## Priority Order

1. **crates.io** - Easy, reaches Rust developers
2. **Homebrew** - Most macOS users expect this
3. **Scoop** - Best for Windows CLI tools
4. **AUR** - Active community, straightforward process
5. **winget/Chocolatey** - Broader Windows coverage
6. **Debian/Ubuntu** - Large user base but high effort

---

## Notes

- Installation instructions will be added to README.md as each package manager becomes available
- GitHub Releases with pre-built binaries serve as the foundation for most package managers
- Some package managers (Snap, Flatpak) may have sandboxing issues that prevent ADB from working correctly
