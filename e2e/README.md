# Jam desktop acceptance tests

This is a standalone Python/pytest project. It drives installed application artifacts
through Appium Mac2 (macOS) or Selenium AT-SPI (Linux/Wayland). It does not import Jam
code, link a Rust library, invoke Store callbacks, or require the Jam checkout.

## Install and run

Install Python 3.11+ and uv, then run `uv sync --locked` in this directory.
The Python dependencies are pinned in `uv.lock`. Run from another checkout by
passing this directory to `uv --project`; no product sources are needed.

On macOS, install full Xcode and grant its automation helper Accessibility access.
Use Appium **3.9.1** with Mac2 **4.6.1** (a compatible major-version pair):

```sh
npm install --global appium@3.9.1
appium driver install mac2@4.6.1
appium driver doctor mac2
appium --port 4723
```

On Linux, install KDE's `selenium-webdriver-at-spi` and its KWin, AT-SPI, Python GI,
and screenshot/input synthesis dependencies. Use its `selenium-webdriver-at-spi-run`
wrapper to run the command below inside its private Wayland session. The driver
must have `KWIN_PID` configured; an Xvfb-only session is not equivalent. Follow
[KDE's setup instructions](https://develop.kde.org/docs/apps/tests/appium/).
Provision software Vulkan (Mesa Lavapipe) or a working physical GPU for the real
terminal renderer. The app must not use a replacement renderer for these tests.

`environments/linux/Dockerfile` builds the AT-SPI driver from an external checkout
of revision `4199d3ea3ccdcaa175b5a06ed3ad8f1af498f38a`
under `environments/linux/upstream`. CI checks it out with credentials disabled;
the image verifies the revision before applying its GTK compatibility patches.
For a local image build, prepare that checkout first:

```sh
git clone --no-checkout https://github.com/KDE/selenium-webdriver-at-spi.git environments/linux/upstream
git -C environments/linux/upstream checkout --detach 4199d3ea3ccdcaa175b5a06ed3ad8f1af498f38a
```

KWin 6.6 comes from Fedora 42's signed archived packages. Its QPainter Wayland
compositor runs inside a private Xvfb host window, allowing the GTK driver proof
without kernel GPU devices. The application and input driver still use Wayland.
Screenshots capture the compositor's actual output window; the OpenGL-only
KWin screenshot API is unavailable with QPainter. GTK bounds combine its
window-relative AT-SPI coordinates with the position read from the owned
compositor. The image uses GTK's built-in Unicode compose input with a US keymap.
Its patch removes an upstream Qt-only tree filter so GTK widgets can be inspected.
`Linux automation proof` exercises real GTK4
controls, a physical slider drag, input, clipboard and screenshots in this environment. This low-level
probe is separate from Jam acceptance and cannot satisfy a product scenario.
Newer KWin versions require a render node or the kernel's `/dev/udmabuf`
device, which hosted CI kernels do not consistently provide. The image removes
KWin's real-time scheduling file capability and needs no host module changes.
This GTK control proof does not establish Jam's GPU renderer compatibility.

```sh
uv run pytest --app /absolute/path/to/Jam.app --platform macos \
  --fixture-bundle /absolute/path/to/fixture-bundle/manifest.json \
  --artifacts-dir /absolute/home/path/jam-e2e-artifacts --junitxml results.xml

uv run pytest --app /absolute/path/to/linux/bin/Jam --platform linux \
  --cli /absolute/path/to/linux/bin/jam --daemon /absolute/path/to/linux/bin/jamd \
  --fixture-bundle /absolute/path/to/fixture-bundle/manifest.json \
  --webdriver-url http://127.0.0.1:4723 --junitxml results.xml
```

The default artifact directory is `e2e/artifacts/`, inside the home-folder checkout,
not host `/tmp`. Failures print full paths. Treat logs as potentially sensitive;
run only against isolated fixture accounts and services, never a personal profile.

No Linux executable exists in Jam yet: supplying a missing app produces setup
errors and a nonzero exit. It never skips, xfails, or substitutes macOS assertions.
MacOS results are not claimed until the actual application is driven on a Mac.

The separate `installed/` smoke suite must receive an unmodified installed stock
release as `--app`, with its own bundled CLI and daemon. Report its results
separately from fixture builds. Clipboard cases save and restore every available
macOS pasteboard representation; snapshots larger than 16 MiB fail before any
clipboard mutation, so a dedicated automation user is preferable.

## Test policy

Every case in `jam_e2e/cases.py` is independently collected on both platforms.
`CASES.md` is its human-readable inventory. The scenario interpreter contains only
external UI, public CLI, Git, and filesystem operations. The driver translates
native accessibility selectors, not expected behavior. Command entry uses a
unique split marker and disables shell echo so echoed commands cannot satisfy
terminal-output assertions. Raw-input cases run a real termios PTY probe.

Missing apps, drivers, display sessions, permissions, services, and resources are
errors. Required cases may not skip or xfail. Selection through `-k`/`-m` returns
nonzero unless `--allow-subset` explicitly identifies a development run. There are
no automatic success-on-retry rules. A full run must not use `-x`/`--maxfail`.

Harness regression tests run independently of application artifacts:

```sh
uv run pytest harness --allow-subset -q
uv run pytest --collect-only -q
```

## Fixture bundle contract

Integration cases require a supplied JSON manifest with `schema: 1` and a
`recipes` mapping keyed by the fixture names in `CASES.md`. A recipe contains
`environment` strings, `setup` arrays of argv arrays, supervised `services`
(`argv` plus a readiness-file path), and `after_services` commands. Substitutions are
`{root}`, `{home}`, `{config}`, `{repo}`, `{socket}`, `{cli}`, `{daemon}`, and
`{bundle}`, and `{python}`. Shell interpolation is never used. Setup commands must be synchronous
provisioners of disposable external worlds; the bundle supervisor owns external
services and tears them down after the run. Recipes cannot override primary
profile/socket isolation. Any command failure is a setup error.

Example structure (paths reference executables supplied by the artifact pipeline):

```json
{"schema":1,"recipes":{"provider-missing":{"setup":[]},
 "remote-connected":{"setup":[["{bundle}/provision-remote","--profile","{root}"]]}}}
```

Supply real disposable SSH/systemd environments for remote cases; fake provider
executables with documented hook/resume protocols for agent cases; controlled
model/usage/review endpoints; and managed fixture-signed application releases for
update cases. Service URLs and signing keys are build configuration of fixture
artifacts, never an E2E control API or a bypass of signature validation. Stock
release smoke tests must be reported separately from fixture-build results.

Create fresh fixture trust and a bundle with:

```sh
python fixtures/prepare_trust.py /absolute/path/to/trust
python fixtures/build_bundle.py /absolute/path/to/bundle --trust /absolute/path/to/trust
```

The artifact pipeline enables the `jam-core/fixture-services` Cargo feature with
`JAM_FIXTURE_ORIGIN=https://localhost:49651`, `JAM_FIXTURE_CA_FILE` pointing to the
absolute `trust/ca.pem`, and `JAM_FIXTURE_UPDATE_KEY_FILE` pointing to the absolute
`trust/update-public-key.hex`. Normal builds have fixed production endpoints and
trust. Fixture builds retain TLS, signature, checksum, and installer validation.
Trust and artifacts must come from the same build; regenerate them together.
Fixture SSH uses the private `HOME/.ssh/config` explicitly, avoiding OpenSSH's
default lookup through the OS account's home directory. Stock builds retain
their normal SSH configuration lookup.

The bundle runs real local HTTPS services and separate provider simulators for
model discovery, saved sessions, hooks, messaging sockets, Codex queue delivery,
acknowledgement, and results. These are deterministic external dependencies, not
live-provider compatibility evidence. Setup uses the shipped public CLI. The
`checks/` suite verifies them against supplied CLI/daemon artifacts:

```sh
uv run pytest checks --cli /installation/jam --daemon /installation/jamd \
  --fixture-bundle /absolute/path/to/bundle/manifest.json
```

Full SSH/systemd worlds and installable update releases are still incomplete.
Their cases remain hard errors when recipes or artifacts are unavailable.
The `Fixture release artifacts` workflow now builds real managed versions
0.0.4 and 0.0.5 from isolated source variants, packages both Linux server
architectures and macOS, and signs their manifests with fixture trust. The
complete desktop workflow consumes this bundle; it exercises the normal
installer with real assets. Successful production of a bundle does not by
itself prove the UI update or rollback scenarios.
`Desktop acceptance` runs the whole case inventory in CI on macOS and Linux;
the absent Linux desktop artifact makes that job fail. `E2E harness` checks only
the automation library and must not be reported as application acceptance.
The early `Desktop acceptance` probe is an explicitly partial development diagnostic and stops
after its first failure. The separate complete acceptance run still runs every
case and does not stop after the first failure.

The basic SSH worlds can use a rootless Podman engine with delegated cgroup v2:

```sh
podman build --memory=512m --jobs=1 -t localhost/jam-e2e-ssh fixtures/ssh
python fixtures/build_bundle.py /absolute/path/to/bundle --trust /absolute/path/to/trust \
  --ssh-image localhost/jam-e2e-ssh --engine /usr/bin/podman \
  --remote-archive /absolute/path/to/real-linux-server.tar.gz
uv run pytest transport --cli /fixture-build/jam --daemon /fixture-build/jamd \
  --fixture-bundle /absolute/path/to/bundle/manifest.json
```

Each SSH case owns a unique container, host keys, login key, known-hosts file,
systemd user manager and remote installation. Runtime containers have a 256 MiB
memory limit and one CPU. The supervisor uses the engine's existing image store
without pulling images; the application continues to use its private profile.

For macOS, the additional QEMU backend boots a real Debian 12 ARM64 guest with
SSH and systemd using TCG, 512 MiB and one CPU. It requires no nested hardware
virtualization. The image URL and SHA-512 digest are pinned in
`fixtures/fetch_vm_image.py`; each case writes only its private overlay and
NoCloud seed. Login and host keys are generated per case, and host key
verification remains strict. Its HTTPS forwarder carries TLS unchanged to the
host's fixture service. Seven real-transport checks passed on macOS in
[run 44915485897](https://github.com/saffronjam/jam/actions/runs/44915485897), including
authentication failures, repair and update deferral while a shell runs.
The full macOS UI workflow now provisions these guests for remote scenarios;
transport proof alone is not a passing remote UI result.

`--remote-previous-archive` supplies a genuinely different older server build
for update deferral. `--remote-next-archive` supplies a newer release for the
required-client-upgrade world. That world's workspace is created with the newer
installation's own CLI over SSH. Dependency checks verify real build versions
and stable sessions; the shared native UI scenario must prove the upgrade gate.

```sh
python fixtures/fetch_vm_image.py /absolute/path/to/debian-arm64.qcow2
python fixtures/build_bundle.py /absolute/path/to/vm-bundle --trust /absolute/path/to/trust \
  --remote-archive /absolute/path/to/real-linux-aarch64-server.tar.gz \
  --vm-image /absolute/path/to/debian-arm64.qcow2 \
  --qemu /absolute/path/to/qemu-system-aarch64 --qemu-img /absolute/path/to/qemu-img \
  --vm-firmware /absolute/path/to/edk2-aarch64-code.fd
```

The VM proof workflow records the exact successful fixture release run it uses.
It verifies reconnect/PID identity, offline failure and disabled linger through
the public CLI and real SSH. This transport proof is separate from UI acceptance.

## Known verification limits

The first driver implementation uses native accessibility roles/names/placeholders.
Missing or ambiguous targets fail. MacOS custom controls and GTK controls may need
ordinary accessibility identifiers/roles added in the app; no hidden test actions
are allowed. The terminal, settings dialog, drag, and key injection paths need
validation on provisioned native sessions. Native IME, visual baselines, installed
upgrade rollback, two-client geometry ownership, and the rest of the migration
catalog still need additional scenarios beyond this initial executable inventory.
Keep the existing Swift, Rust, renderer, and installer tests throughout.
