import Foundation

struct ServerStatusTests {
  static func run() throws {
    func status(
      running: Bool = true, compatible: Bool = true, available: Bool = true, terminals: Int? = 0
    ) -> ServerStatus {
      ServerStatus(
        running: running, compatible: compatible, build: "old",
        availableBuild: available ? "new" : nil, updateAvailable: true, terminals: terminals,
        error: nil)
    }
    assert(status().readyForScheduledUpdate)
    assert(!status(terminals: 1).readyForScheduledUpdate)
    assert(!status(terminals: nil).readyForScheduledUpdate)
    assert(!status(compatible: false).readyForScheduledUpdate)
    assert(!status(running: false).readyForScheduledUpdate)
    assert(!status(available: false).readyForScheduledUpdate)
    let offline = MachineActions(
      connected: false, connecting: false, busy: false, checking: false, status: status())
    assert(
      offline.canReconnect && !offline.canDisconnect && !offline.canUpdate && !offline.canRestart
        && !offline.canCheck)
    assert(!offline.needsUpdate, "Cached status must not mark an offline machine as connected")
    let outdated = MachineActions(
      connected: true, connecting: false, busy: false, checking: false, status: status())
    assert(
      outdated.needsUpdate && outdated.canUpdate && outdated.canRestart && !outdated.canReconnect)
    let incompatible = MachineActions(
      connected: false, connecting: false, busy: false, checking: false,
      status: status(compatible: false))
    assert(
      incompatible.needsUpdate && incompatible.canUpdate && incompatible.canRestart
        && incompatible.canCheck && !incompatible.canReconnect,
      "An old protocol must not lock the user out of server upgrades")
    let unreachable = MachineActions(
      connected: false, connecting: false, busy: false, checking: false,
      status: status(running: false, compatible: false))
    assert(unreachable.canReconnect && !unreachable.canUpdate && !unreachable.canRestart)
    let connecting = MachineActions(
      connected: false, connecting: true, busy: false, checking: false, status: nil)
    assert(!connecting.canReconnect && connecting.canDisconnect && !connecting.canRestart)
    let current = MachineActions(
      connected: true, connecting: false, busy: false, checking: false,
      status: ServerStatus(
        running: true, compatible: true, build: "same", availableBuild: "same",
        updateAvailable: false, terminals: 1, error: nil))
    assert(!current.needsUpdate && !current.canUpdate && current.canRestart && current.canCheck)
    var unlinked = status()
    unlinked.managementAvailable = false
    assert(!unlinked.readyForScheduledUpdate)
    let live = MachineActions(
      connected: true, connecting: false, busy: false, checking: false, status: unlinked)
    assert(live.canCheck && !live.canTakeover && !live.canRestart && !live.canUpdate)
    let failed = ServerStatus(
      running: true, compatible: false, build: nil, availableBuild: nil, updateAvailable: true,
      terminals: nil, error: "Local server is not responding", state: "inspection_failed",
      managementAvailable: false)
    let retryable = MachineActions(
      connected: false, connecting: false, busy: false, checking: false, status: failed)
    assert(retryable.canCheck && !retryable.canTakeover && !retryable.canRestart)
    assert(!retryable.incompatibleServer && !retryable.needsUpdate && !retryable.canUpdate)
    assert(!failed.blocked && failed.label == "Server unavailable")
    for value in [
      "unsupported", "takeover_required", "recovery_required", "client_update_required",
    ] {
      let payload =
        "{\"running\":true,\"compatible\":false,\"update_available\":true,\"state\":\"" + value
        + "\",\"can_takeover\":true}"
      let blocked = try JSONDecoder().decode(ServerStatus.self, from: Data(payload.utf8))
      let actions = MachineActions(
        connected: false, connecting: false, busy: false, checking: false, status: blocked)
      assert(blocked.blocked && !actions.canReconnect && !actions.canUpdate && !actions.canRestart)
      assert(blocked.label != nil)
      for connection in [RemoteConnectionState.disconnected, .retryWaiting, .connecting] {
        let presentation = MachinePresentation(
          status: blocked, connection: connection, phase: nil, hasError: false)
        assert(
          presentation.label == blocked.label,
          "Compatibility takes precedence over transport state on both surfaces")
        assert(!presentation.isAttempting)
        assert(presentation.warning)
      }
      let installing = MachinePresentation(
        status: blocked, connection: .disconnected, phase: "installing", hasError: false)
      assert(installing.label == "Installing…" && installing.isAttempting)

      assert(
        !actions.needsUpdate, "A blocking server action takes precedence over update availability")
      if value == "unsupported" || value == "takeover_required" {
        assert(blocked.label == "Takeover required")
      }
    }
    print("Server scheduling passed: only a verified idle server can update automatically.")
  }
}
