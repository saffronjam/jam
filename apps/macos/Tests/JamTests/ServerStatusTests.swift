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
    print("Server scheduling passed: only a verified idle server can update automatically.")
  }
}
