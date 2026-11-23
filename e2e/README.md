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
python environments/macos/prepare_driver.py "${APPIUM_HOME:-$HOME/.appium}/node_modules/appium-mac2-driver"
appium --port 4723
```

Native Unicode and IME input also need event-posting permission for the terminal
or runner that invokes the Swift helpers. Run `swift native/macos/environment.swift` and
check `post_event_access`. Missing access fails normally; the suite does not
request or grant permissions.

Visual cases also require `tesseract` and its English language data (`brew install
tesseract` on macOS; `tesseract` plus `tesseract-langpack-eng` in the pinned Fedora
image). Pillow is pinned in the Python lockfile. The OCR command follows
[Tesseract's documented CLI](https://tesseract-ocr.github.io/tessdoc/Command-Line-Usage).

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
Native composition cases use the US/ABC dead key on macOS and GTK's Unicode
preedit on Linux. They assert no PTY bytes before commit or after cancellation;
ordinary committed-text injection does not satisfy these cases. Mac runners
need the US/ABC input source. French/German shared layout cases are implemented,
passed against Jam in
[run 45049001771](https://github.com/saffronjam/jam/actions/runs/45049001771).
The independent AppKit text-field probe
showed XCTest drops Option punctuation before the app receives any event.
The driver now discovers physical Option combinations from the selected native
layout and sends actual balanced modifier/key events. The probe retains native
events to verify this correction before attributing failures to Jam.
`IME-003/004` select the actual Japanese Romaji source on macOS and an owned
IBus/Mozc instance on Linux. Physical `kana` input must produce no PTY bytes
during preedit, commit exactly `かな`, or cancel without leaking input. Native
preedit screenshots, input-source observations and service cleanup receipts are
retained. Linux starts with an ordinary US engine so the raw PTY observer can
launch before switching to Japanese. The observer allows a bounded 60 seconds
for native source switching; its exact byte assertions remain unchanged.
The private Mozc configuration uses its documented
[Hiragana engine mode](https://github.com/google/mozc/blob/master/docs/configurations.md#fixed-composition-mode-per-engine).
The IBus service uses an explicit owned socket, never `--replace`, and only
cleans processes bearing its exact private-profile marker. Separate AppKit/GTK
text-field controls exercise native commit/cancellation. Both Japanese Jam
cases passed in [run 45058420456](https://github.com/saffronjam/jam/actions/runs/45058420456),
including exact committed UTF-8, no preedit PTY bytes, single-Escape cancellation
and ordinary typing recovery. Real GTK IBus/Mozc commit/cancellation,
ordinary input recovery and owned-service cleanup passed in
[run 45050907001](https://github.com/saffronjam/jam/actions/runs/45050907001).
macOS's Japanese mode can have an enabled preference while its parent method is
disabled. The native adapter records and enables both prerequisites, then
restores each changed source in reverse order and reads back its disabled state.
For input methods, the preference helper owns a temporary native field/key
window during selection, then returns focus to the same application. This
avoids changing only the global menu source while leaving the active input
context on its previous mode. The standard AppKit probe records the editor's
actual marked-text state; an accessible field value can include marked text.
The product scenarios continue to assert exact raw PTY bytes independently.
The native probe showed XCTest temporarily selects ABC while synthesizing text,
so Japanese-mode input uses Core Graphics
[virtual-key down/up events](https://developer.apple.com/documentation/coregraphics/cgevent/init(keyboardeventsource:virtualkey:keydown:))
instead. This path requires existing event-posting permission, validates the
owned foreground process and unchanged source, and never injects Unicode text.
Physical native preedit, exact Hiragana commit and ordinary typing recovery
passed in [run 45057493924](https://github.com/saffronjam/jam/actions/runs/45057493924).
In the standard field both Escape and Clear retained marked Hiragana on this
source. Its erasure control now sends two physical Backspaces, requires the
intermediate marked `か`, then empty/unmarked text and ordinary typing recovery.
This erasure control passed in
[run 45059539869](https://github.com/saffronjam/jam/actions/runs/45059539869).
Jam's passing single-Escape/zero-PTY assertion is unchanged.
Resize scenarios use a real XCTest grip drag on macOS and the owned KWin
window manager on Linux. An independent PTY program records kernel `SIGWINCH`
events and terminal rows/columns, while the suite checks that shell PIDs and
layout identities survive. Native geometry and long-title cases passed on macOS
in [run 44931567001](https://github.com/saffronjam/jam/actions/runs/44931567001).
The GTK proof below includes native resize and later
hit testing as well as a scrolled-field edit.
`Linux automation proof` exercises real GTK4
controls, a physical slider drag, input, clipboard and screenshots in this environment. This low-level
probe is separate from Jam acceptance and cannot satisfy a product scenario.
The complete GTK driver proof passed in
[run 44928918319](https://github.com/saffronjam/jam/actions/runs/44928918319), including
physical wheel scrolling, native composition, Unicode, resizing and later hit
testing. Its wheel patch sends timed frames and explicit stop events.
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

Jam has no Linux desktop executable yet: supplying a missing app produces setup
errors and a nonzero exit. It never skips, xfails, or substitutes macOS assertions.
macOS results are not claimed until the actual application is driven on a Mac.
The recorded pre-integration Linux run executed all 251 shared cases and reported 251
ordinary missing-client setup errors, zero skips, and exit status 1. Its 18
external dependency checks and three native GTK/Mozc controls pass. See
[run 45167944167](https://github.com/saffronjam/jam/actions/runs/45167944167).
The macOS job in that run passed all 251 scenarios, with zero errors, failures
or skips, in 10,925.529 seconds. Its 18 dependency checks, five native probes and
three unmodified stock smokes also passed. See the
[macOS job](https://github.com/saffronjam/jam/actions/runs/45167944167/job/137670091062).
It used framework revision `4f9d9e7` and signed product fixtures from `6caa427`.
That baseline predates integration of `main` at `004cf26`; integrated builds
require fresh native validation. The workflow remains red because the Linux
client is absent, as required by the shared contract.
`REMOTE-006` lost its Disconnect menu target during startup; the case now proves
a real remote-terminal round trip before that action. `SERVER-001` failed when
the independent SSH observer inherited the guest boot poll's three-second
handshake limit. The observer now uses ten seconds and one connection attempt
within its unchanged 20-second command deadline. Host-key/authentication policy
and exact peer-state assertions remain intact. The effective policy is checked
through OpenSSH's own config parser; see
[ConnectTimeout](https://man.openbsd.org/ssh_config#ConnectTimeout).
An earlier run also exposed disabled Reconnect clicks and initial project
lookups that expired before healthy startup completed. Authentication uses the
enabled native-menu path, and remote shell scenarios observe the project for up
to 60 seconds before sending input. These waits observe readiness and never
replay input. Remote failures retain SSH service logs when reachable.
The whole-job budget is four hours, with per-case progress and immediate failure
tracebacks. Readiness waits are bounded observations; input actions are never replayed.

The separate `installed/` smoke suite must receive an unmodified installed stock
release as `--app`, with its own bundled CLI and daemon. Report its results
separately from fixture builds. Published v0.0.4 has also produced startup CLI
`EAGAIN` errors, with diagnostics capturing a native SQLite mutex stall. That
failure is retained in
[run 45159153396](https://github.com/saffronjam/jam/actions/runs/45159153396).
Current fixture builds use the updated SQLite dependency. These failures remain
ordinary errors, with no automatic retry.
Clipboard cases save and restore every available
macOS pasteboard representation; snapshots larger than 16 MiB fail before any
clipboard mutation, so a dedicated automation user is preferable.

## Test policy

The daemon-restart case observes a new live PID with the same build and protocol
from the private runtime receipt after the UI action. It retains the before/after
receipt and checks that the command's execution file stays unchanged for two
seconds. A visible Terminal label alone cannot satisfy this restart check, and
its observer never invokes a CLI command that could start a missing daemon.

The connected-client Disconnect case first executes a real remote-terminal round
trip, establishing that this UI has connected before its menu action. It then
requires disabled Disconnect, enabled Reconnect and unchanged peer sessions.
The strengthened case passed in
[run 45159183602](https://github.com/saffronjam/jam/actions/runs/45159183602).

Every case in `jam_e2e/cases.py` is independently collected on both platforms.
`CASES.md` is its human-readable inventory. The scenario interpreter contains only
external UI, public CLI, Git, and filesystem operations. The driver translates
native accessibility selectors, not expected behavior. Command entry uses a
unique split marker and disables shell echo so echoed commands cannot satisfy
terminal-output assertions. Raw-input cases run a real termios PTY probe.

Multiple-client cases launch distinct native processes sharing one private
profile and daemon. On macOS they use separate complete bundle copies and
XCTest's path-specific application selection; Linux uses separate AT-SPI sessions
and activates each owned window through KWin. Each switch verifies its process
identity, and failures retain each client's tree and screenshot. The scenarios
cover terminal continuity, settings conflicts and converging tab edits. All
three passed against Jam on macOS in
[run 44933667395](https://github.com/saffronjam/jam/actions/runs/44933667395).
The low-level GTK two-process proof passed in
[run 44932450840](https://github.com/saffronjam/jam/actions/runs/44932450840), with
distinct PIDs 95 and 441, independent fields/counters and a usable primary after
closing the secondary. This remains a separate automation-tool proof.

Visual cases read native terminal screenshots through an independent OCR engine,
check the light/dark background, and repeat after client relaunch or resize.
The expected phrase is emitted as octal shell bytes, preventing an echoed input
line from satisfying the image assertion. Native pixels, the enlarged OCR input
and every OCR observation are retained for successful and failed visual cases.
These are cross-platform legibility/continuity checks, not shared pixel-perfect
reference images. All three passed against actual Jam rendering on macOS in
[run 44933667395](https://github.com/saffronjam/jam/actions/runs/44933667395).
OCR inputs add a small border around the terminal crop and invert dark images,
following [Tesseract's image-quality guidance](https://tesseract-ocr.github.io/tessdoc/ImproveQuality.html).
Original pixels and exact expected-text assertions remain unchanged.

`SCALE-001` exercises native output scaling, real terminal pixels, physical hit
testing, PTY `SIGWINCH`, preserved layout/PIDs and restoration. It starts on the
isolated scale-1 desktop. On macOS the app moves to an owned Retina virtual
display using the CoreGraphics interfaces exercised by
[Chromium's native display tests](https://chromium.googlesource.com/chromium/src/+/HEAD/ui/display/mac/test/virtual_display_util_mac.mm).
On Linux the private KWin output changes to scale 1.25 through
[KScreen's native command](https://github.com/KDE/libkscreen/blob/master/src/doctor/main.cpp).
The helper process has a bounded lifetime and cleanup removes its own display.
The complete Jam scale scenario passed in
[run 45050932847](https://github.com/saffronjam/jam/actions/runs/45050932847).

`MOTION-001` opens and dismisses the command palette and settings with native
reduced motion enabled, then verifies rendered terminal output and unchanged
layout/shell PIDs. Linux uses `gtk-enable-animations=false` in the case's private
GTK4 configuration; the actual `Gtk.Settings` value is read in that Wayland
environment. macOS reads `NSWorkspace.accessibilityDisplayShouldReduceMotion`
and fails if the native runner has not enabled it. No host preference is changed.
Native setting observations are retained. This checks usable workflows under
reduced motion; it does not measure individual animation timelines.

The reduced-motion product case passed on macOS in
[run 45048085210](https://github.com/saffronjam/jam/actions/runs/45048085210).
The native GTK control also observed the actual disabled-animation setting and
passed in [run 45048026487](https://github.com/saffronjam/jam/actions/runs/45048026487).

The concurrency coverage includes external-edit/draft conflicts (`CFG-022/023`),
two-client config/layout convergence (`MULTI-002/003`), changes during upgrade
confirmation (`UPDATE-013`), and cancellation before a scheduled server update
(`SERVER-008`). `RACE-001/002` add rapid query replacements in one physical
keyboard stream over 6,000 terminal history lines, then require the final count
to remain stable or the closed search to stay closed with working terminal focus.
`RACE-003` switches tab scope during query replacement and checks independent
zero/one-match histories. These exercise real asynchronous UI behavior without
controlling internal scheduling. `RACE-001/002` passed against Jam in
[run 45050932847](https://github.com/saffronjam/jam/actions/runs/45050932847).
`RACE-003` observes the closed find bar before requiring working terminal input
after tab switching. The complete case passed in
[run 45053731749](https://github.com/saffronjam/jam/actions/runs/45053731749).

OpenSSH's macOS password/host-key dialog is a native accessory process from the
same bundle as the main client. The adapter observes the owned helper PID and
window through the OS and reads its real title pixels. Cancel/default actions
use native Escape/Return equivalents and must close that exact helper PID.
UI acceptance verifies actual SSH results and unchanged peer sessions. Password
acceptance covers the separate inspection, socket discovery and forwarding SSH
connections.

After authentication, the adapter activates the original main PID. XCTest's
same-bundle identity cache can retain the exited helper, clear an explicit PID
binding, or hang on an element click. Prompt cases therefore own app
lifetime outside XCTest: disconnect with `skipAppKill`, start a fresh automation
runner on a new native port without an app-launch capability, then bind to the
independently verified main PID. The main PID must remain identical throughout;
the case and its actions are never replayed. Native application queries validate
the live PID and canonical bundle before restoring its binding. Attached clients
use observed accessible bounds and desktop pointer/keyboard events, with a
read-only original-PID/foreground check before input. Cleanup requests normal
native termination. A terminated-app control requires UI reads to fail without
relaunching the app.

All twelve native controls passed in
[run 45167895111](https://github.com/saffronjam/jam/actions/runs/45167895111),
including post-accessory input through canonical and symlink paths, exact field
text, unchanged main PID, terminated-app rejection, and exactly one native menu
callback per selection. Native menu selection uses one physical desktop click
at the accessible target bounds.

The diagnostic runs from an extracted copy containing only this E2E project,
outside the product checkout. Its receipt verifies the working directory,
cases/driver/steps module origins and private Python environment. All twelve
native controls, five cases against the published application and three
installed-stock smokes pass
from that copy. The host checkout remains available on disk; these are native
runtime independence checks, separate from the complete shared baseline.
Mac2 can report a replaced full-text element as an invalid element state with
XCTest code 10008 and a no-matches message. The adapter classifies only that
specific read error as stale, so the existing bounded observer locates the new
element. Other invalid states propagate, and input actions are never replayed.

All four SSH password/host-key acceptance and cancellation cases, plus remote
startup and the persistence warning, passed against signed fixtures with zero
failures, errors or skips in
[run 45166154876](https://github.com/saffronjam/jam/actions/runs/45166154876).
Password acceptance includes the real remote shell and preserved workspace.
Background refresh reuses the authenticated tunnel, validates the running server
release and reconnects after a failed cached connection. Interactive forwarding
waits for authentication, SSH exit or cancellation; time spent answering a native
prompt no longer consumes the batch connection's five-second forwarding budget.
OpenSSH's transport timeout remains unchanged. The existing core subprocess
regression exercises an eight-second authentication delay and cancellation.
Dialog observation reads fresh bounds from the same owned helper PID for each
OCR observation, following the native window's opening animation.
Ordinary committed Unicode uses native Quartz keyboard down/up events containing
complete UTF-16 scalars, including supplementary characters. It keeps the native
input source unchanged. The independent AppKit control checks exact received text
and retains its actual key events. Japanese IME scenarios keep their physical
virtual-key composition path and exact raw PTY assertions.

The native Mac2 patches are pinned to version 4.6.1 and exact original/
patched SHA-256 hashes. Explicit `desktop-*` W3C event sources use XCTest's
existing physical event synthesizer without resolving the ambiguous AUT;
desktop streams reject element-relative origins. Ordinary AUT actions keep
their original path. Explicit process attachment validates the live PID and its
canonical bundle path before binding XCTest to its process ID; it never launches
an app. Unsupported XCTest versions fail. These patches change the automation
tool, not the product.

Selection cases locate the actual rendered word in a screenshot and drag
across its glyphs with native pointer input. They verify both automatic clipboard copy
and disabled automatic copy, followed by explicit Copy of the exact word.
Both setting-effect cases passed on macOS in
[run 44939728419](https://github.com/saffronjam/jam/actions/runs/44939728419).
That run failed its separate visual-resize OCR assertion. The extended GTK
OCR targeting and clipboard selection proof passed in run 44936942628.

The additional shared cases check that focus transfers PTY geometry between
two differently sized clients without background resize churn, and that
disabling checkout deletion hides its action after relaunch. Re-enabling it
must restore the action while cancelling its confirmation preserves files and
running shells. Both deletion cases passed in
[run 44938229985](https://github.com/saffronjam/jam/actions/runs/44938229985).
Focused-client sizing passed with fresh signed products in
[run 45070980018](https://github.com/saffronjam/jam/actions/runs/45070980018).
The selected pane waits for its normal focus request before reclaiming PTY
size. The unchanged case verifies real kernel dimensions, client process
identity and absence of background resize churn.

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

Provider fixtures need a POSIX C compiler (`cc`). Each native provider-named
launcher supervises its Python protocol peer with ordinary stdio and signal
forwarding. This preserves the OS process identity used for account discovery;
macOS resolves Python interpreter aliases to Python itself. A real-process
harness check verifies identity, protocol delivery and child cleanup.

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

Seven selected upgrade/server UI cases passed in
[run 45032406705](https://github.com/saffronjam/jam/actions/runs/45032406705),
including `UPDATE-002`, `REMOTE-007`, `SERVER-001` through `SERVER-004`, and
`SERVER-006`. These focused results do not establish the complete current
macOS baseline. Missing recipes or installation artifacts remain hard setup errors.
macOS fixture builds use the pinned repository toolchain on `macos-26`, while
the native desktop scenarios continue to exercise the supported `macos-15`
runtime.

The `Fixture release artifacts` workflow builds real managed versions
0.0.4 and 0.0.5 from isolated source variants, packages both Linux server
architectures and macOS, and signs their manifests with fixture trust. A separate
archived candidate compiles a deliberately failing daemon startup and is signed
through the same packaging path. Only the rollback world serves that candidate;
the normal next-version world still serves the working release. The rollback
case checks actual candidate execution, automatic restoration, the old installed
version, layout identity, command non-replay and a usable shell. It passed natively
in [run 44929065283](https://github.com/saffronjam/jam/actions/runs/44929065283).
The normal successful upgrade, genuine rollback and new-session consent refusal
all passed in
[run 44939260667](https://github.com/saffronjam/jam/actions/runs/44939260667).
The complete desktop workflow consumes this bundle and exercises the normal installer
with real assets. Successful production of a bundle does not by
itself prove the UI update or rollback scenarios.
`Desktop acceptance` runs the whole case inventory in CI on macOS and Linux;
the absent Linux desktop artifact makes that job fail. `E2E harness` checks only
the automation library and must not be reported as application acceptance.
The early `Desktop acceptance` probe is an explicitly partial development diagnostic and stops
after its first failure. The separate complete acceptance run still runs every
case and does not stop after the first failure.
Linux acceptance uses the same private desktop through `Dockerfile.acceptance`
and `run-acceptance.sh`, including real QEMU SSH guests. It matches the runner's
UID so private release trust and artifacts remain readable inside the container.
Release publishing requires the complete shared acceptance workflow alongside
the existing build, signature and rollback checks. Missing Linux functionality
therefore blocks release; a passing driver or harness check cannot override it.
For an independent complete verification queue while an older run remains
active, dispatch `Desktop acceptance` with a nonempty `proof_lane`. It uses the
same immutable checkout, both operating systems, full inventory and failure
policy. Ordinary push and release queues keep their existing group.
An explicit manual diagnostic can also set `fixture_run` to a completed successful
`Fixture release artifacts` run. The workflow records that producer's source head
in each platform's evidence and still runs every case on both operating systems.
Use this for test-only changes; it does not prove product changes absent from the
selected artifacts. Normal push and release runs build from their own revision.

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
host's fixture service. All nine real-transport checks passed on macOS in
[run 44924628748](https://github.com/saffronjam/jam/actions/runs/44924628748), including
authentication failures, repair, update deferral, a newer server and an actual
incompatible wire protocol.
The full macOS UI workflow now provisions these guests for remote scenarios;
transport proof alone is not a passing remote UI result.

`--remote-previous-archive` supplies a genuinely different older server build
for update deferral. `--remote-next-archive` supplies a newer release for the
required-client-upgrade world. That world's workspace is created with the newer
installation's own CLI over SSH. Dependency checks verify real build versions
and stable sessions; the shared native UI scenario must prove the upgrade gate.
`--remote-incompatible-archive` supplies a separately compiled daemon with a
different wire identity. The fixture producer changes that declaration only in
an archived source variant and packages it through the normal server builder;
production source and protocol selection remain unchanged.

```sh
python fixtures/fetch_vm_image.py /absolute/path/to/debian-arm64.qcow2
python fixtures/build_bundle.py /absolute/path/to/vm-bundle --trust /absolute/path/to/trust \
  --remote-archive /absolute/path/to/real-linux-aarch64-server.tar.gz \
  --vm-image /absolute/path/to/debian-arm64.qcow2 \
  --qemu /absolute/path/to/qemu-system-aarch64 --qemu-img /absolute/path/to/qemu-img \
  --vm-firmware /absolute/path/to/edk2-aarch64-code.fd
```

The macOS development probe accepts `remote_worlds=true` with an explicit
`fixture_run` to provision these real guests for selected native UI cases. These
partial diagnostics remain separate from the complete acceptance gate. Native
setup handles only the hosted runner's specific Local Network consent through
XCTest and records its XML/screenshot. Product authentication prompts remain
scenario actions. Primary failures and teardown failures have separate captures.

The VM proof workflow records the exact successful fixture release run it uses.
It verifies reconnect/PID identity, offline failure and disabled linger through
the public CLI and real SSH. This transport proof is separate from UI acceptance.

## Known verification limits

The drivers use native accessibility roles, names and placeholders.
Missing or ambiguous targets fail. MacOS custom controls and GTK controls may need
ordinary accessibility identifiers/roles added in the app; no hidden test actions
are allowed. Terminal input, settings, native dragging, clipboard and composition have
selected native proofs. macOS marked-text commit/cancellation and ordinary
Escape passed against the actual fix in
[run 44928235462](https://github.com/saffronjam/jam/actions/runs/44928235462).
GTK scrolling, resizing, fractional scaling, later hit testing and real Japanese
composition pass as automation-tool checks. French/German input and reduced
motion have passing Jam proofs.
Real Japanese commit/cancellation and SSH cancellation also pass against Jam.
Focused-client geometry and password acceptance have passing native evidence.
All four SSH authentication cases pass in focused native verification. All three
remote update scenarios pass against the status-refresh fix in
[run 45113629237](https://github.com/saffronjam/jam/actions/runs/45113629237).
The recorded pre-integration macOS baseline passed all 251 cases. A Linux Jam
application is absent and acceptance must fail.
Keep the existing Swift, Rust, renderer, and installer tests throughout.
