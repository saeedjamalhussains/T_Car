# Building and installing T-Car

T-Car ships three things from one workspace:

| | built by |
| --- | --- |
| the desktop app (`T-Car.app`, `.msi`/`.exe`, `.deb`/`.rpm`/`.AppImage`) | `cargo tauri build` in `apps/desktop` |
| the `tcar` command line tool | `cargo build --release -p tcar-cli` |
| the `tcar` Rust library | `cargo build -p tcar-core` |

**Each desktop bundle must be built on the OS it targets.** The Tauri shell
links against a system webview — WebKit on macOS, WebView2 on Windows,
webkit2gtk on Linux — and the Windows installer needs the MSVC toolchain, so
there is no supported way to cross-build a GUI bundle from one OS to another.
Build on the target OS, or let CI do all three
([`.github/workflows/release.yml`](../.github/workflows/release.yml)).

Common first steps on every platform:

```bash
# 1. Rust (https://rustup.rs)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # macOS / Linux
#    Windows: download and run https://win.rustup.rs/x86_64

# 2. The Tauri CLI, once
cargo install tauri-cli --version "^2" --locked
```

---

## Windows

### Prerequisites

1. **Visual Studio Build Tools** — install the *Desktop development with C++*
   workload (this provides the MSVC linker and the Windows SDK):
   <https://visualstudio.microsoft.com/visual-cpp-build-tools/>
2. **WebView2 Runtime** — preinstalled on Windows 11 and current Windows 10.
   If missing: <https://developer.microsoft.com/microsoft-edge/webview2/>
3. **Rust** with the MSVC toolchain (the rustup default):
   `rustup default stable-x86_64-pc-windows-msvc`

### Build

```powershell
git clone <this repo> ; cd t-car
cargo install tauri-cli --version "^2" --locked
cd apps\desktop
cargo tauri build
```

Output in `target\release\bundle\`:

| file | what it is |
| --- | --- |
| `msi\T-Car_2.0.0_x64_en-US.msi` | Windows Installer package |
| `nsis\T-Car_2.0.0_x64-setup.exe` | NSIS installer (`--bundles nsis` alone) |
| `..\..\release\T-Car.exe` | the raw unpackaged executable |

Build just one kind with `cargo tauri build --bundles msi` or `--bundles nsis`.

### The `tcar` command

```powershell
cargo build --release -p tcar-cli
target\release\tcar.exe lookup "+91 98765 43210"
```

Install it onto your PATH with `cargo install --path crates\tcar-cli`
(lands in `%USERPROFILE%\.cargo\bin`).

Config lives at `%USERPROFILE%\.config\tcar\config.json`; override the location
with the `TCAR_CONFIG` environment variable.

### Notes

- The installers are unsigned, so SmartScreen shows "Windows protected your
  PC" on first run → **More info → Run anyway**. Sign with
  `signtool` / a Tauri `bundle.windows.certificateThumbprint` for real
  distribution.
- Cross-building `tcar.exe` (the CLI only, no GUI) from macOS or Linux is
  possible with mingw-w64: see `scripts/build-windows-cli.sh`.

---

## Linux

### Prerequisites

Debian / Ubuntu (22.04+):

```bash
sudo apt update
sudo apt install -y libwebkit2gtk-4.1-dev libappindicator3-dev \
  librsvg2-dev patchelf libssl-dev build-essential curl file
```

Fedora / RHEL:

```bash
sudo dnf install -y webkit2gtk4.1-devel libappindicator-gtk3-devel \
  librsvg2-devel patchelf openssl-devel @development-tools
```

Arch:

```bash
sudo pacman -S --needed webkit2gtk-4.1 libappindicator-gtk3 librsvg \
  patchelf openssl base-devel
```

> On distributions still shipping webkit2gtk **4.0** (Ubuntu 20.04, Debian 11),
> install `libwebkit2gtk-4.0-dev` and pin Tauri to a build configured for it,
> or build in a newer container — see `scripts/build-linux.sh`.

### Build

```bash
git clone <this repo> && cd t-car
cargo install tauri-cli --version "^2" --locked
cd apps/desktop
cargo tauri build
```

Output in `target/release/bundle/`:

| file | what it is |
| --- | --- |
| `deb/t-car_2.0.0_amd64.deb` | Debian/Ubuntu package |
| `rpm/t-car-2.0.0-1.x86_64.rpm` | Fedora/RHEL package |
| `appimage/t-car_2.0.0_amd64.AppImage` | portable, runs anywhere |

```bash
sudo dpkg -i target/release/bundle/deb/t-car_2.0.0_amd64.deb   # or
chmod +x target/release/bundle/appimage/*.AppImage && ./target/release/bundle/appimage/*.AppImage
```

Pick one format with `cargo tauri build --bundles deb` (or `rpm`, `appimage`).

### The `tcar` command

```bash
cargo build --release -p tcar-cli
./target/release/tcar lookup 801-360-4427
cargo install --path crates/tcar-cli          # onto ~/.cargo/bin
```

Config lives at `~/.config/tcar/config.json` (`TCAR_CONFIG` overrides it).

### Notes

- AppImage needs FUSE; on systems without it, run with `--appimage-extract-and-run`.
- Wayland users seeing a blank window: try `WEBKIT_DISABLE_COMPOSITING_MODE=1 t-car`.
- Building Linux bundles from a Mac: `./scripts/build-linux.sh` does it in a
  Docker container (Docker Desktop must be running).

---

## macOS

```bash
xcode-select --install
cargo install tauri-cli --version "^2" --locked
cd apps/desktop && cargo tauri build
```

Output: `target/release/bundle/macos/T-Car.app` and
`target/release/bundle/dmg/T-Car_2.0.0_aarch64.dmg`. Add
`--target x86_64-apple-darwin` for an Intel build, or
`--target universal-apple-darwin` for a fat binary.

The bundle is ad-hoc signed, not notarized, so the first launch needs
**right-click → Open**.

**If the DMG step fails** with `Not authorised to send Apple events to Finder
(-1743)`: Tauri's DMG script asks Finder to lay out the window. Either grant
your terminal control of Finder in *System Settings → Privacy & Security →
Automation*, or build the `.app` alone and make the DMG yourself:

```bash
cargo tauri build --bundles app
STAGE=$(mktemp -d)/T-Car && mkdir -p "$STAGE"
cp -R target/release/bundle/macos/T-Car.app "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "T-Car" -srcfolder "$STAGE" -ov -format UDZO \
  target/release/bundle/dmg/T-Car_2.0.0_aarch64.dmg
```

---

## All three at once (CI)

[`.github/workflows/release.yml`](../.github/workflows/release.yml) builds
macOS (arm64 + Intel), Windows (`.msi` + `.exe`), and Linux
(`.deb`/`.rpm`/`.AppImage`) on their own runners, plus the `tcar` CLI for each.

- **Manually:** Actions tab → *build* → *Run workflow*, then download the
  per-platform artifacts.
- **On a tag:** `git tag v2.0.0 && git push --tags` attaches every bundle to a
  draft GitHub release.

## Mobile

The same `apps/desktop` crate targets phones:

```bash
cd apps/desktop
cargo tauri ios init && cargo tauri ios build       # macOS + Xcode required
cargo tauri android init && cargo tauri android build   # Android SDK/NDK required
```
