import Foundation

struct ServerStatus: Decodable {
  let running: Bool
  let compatible: Bool
  let build: String?
  let availableBuild: String?
  let updateAvailable: Bool
  let terminals: Int?
  let error: String?
  var state: String? = nil
  var canTakeover: Bool? = nil
  var minimumVersion: String? = nil
  var managementAvailable: Bool? = nil
  var blocked: Bool {
    ["unsupported", "takeover_required", "recovery_required", "client_update_required"].contains(
      state ?? "")
  }
  var label: String? {
    switch state {
    case "recovery_required": return "Recovery required"
    case "unsupported", "takeover_required": return "Takeover required"
    case "client_update_required": return "Update Jam required"
    case "inspection_failed": return "Server unavailable"
    default: return nil
    }
  }
  var readyForScheduledUpdate: Bool {
    !blocked && managementAvailable != false && running && compatible && availableBuild != nil
      && updateAvailable && terminals == 0
  }
  enum CodingKeys: String, CodingKey {
    case running, compatible, build, terminals, error, state
    case canTakeover = "can_takeover"
    case minimumVersion = "minimum_version"
    case managementAvailable = "management_available"
    case availableBuild = "available_build"
    case updateAvailable = "update_available"
  }
}
/// Both sidebar surfaces use this precedence, including stale transport results.
struct MachinePresentation: Equatable {
  let label: String?
  let isAttempting: Bool
  let warning: Bool

  init(status: ServerStatus?, connection: RemoteConnectionState, phase: String?, hasError: Bool) {
    warning = status?.blocked == true || status?.state == "inspection_failed" || hasError
    isAttempting = phase != nil || (!warning && connection.isAttempting)
    if let phase {
      label = phase.capitalized + "…"
    } else if hasError {
      label = "Needs attention"
    } else if let statusLabel = status?.label {
      label = statusLabel
    } else {
      label = connection.label
    }
  }
}

struct ServerAction: Identifiable {
  enum Kind { case update, restart, takeover, inspect }
  let endpoint: String
  let name: String
  let kind: Kind
  var id: String { endpoint + ":" + String(describing: kind) }
}

struct MachineActions {
  let connected: Bool
  let connecting: Bool
  let busy: Bool
  let checking: Bool
  let status: ServerStatus?
  // A reachable server with an older protocol cannot establish an app
  // connection. Keep its explicit upgrade/restart actions available.
  var incompatibleServer: Bool { status?.running == true && status?.compatible == false }
  private var manageable: Bool {
    (connected || incompatibleServer) && status?.managementAvailable != false
  }
  var canTakeover: Bool { status?.canTakeover == true && !busy && !checking }
  var canReconnect: Bool {
    status?.blocked != true && !connected && !incompatibleServer && !connecting && !busy
  }
  var canDisconnect: Bool { (connected || connecting) && !busy }
  var canCheck: Bool {
    (connected || incompatibleServer || status?.state == "inspection_failed") && !busy && !checking
  }
  var needsUpdate: Bool {
    manageable && status?.blocked != true
      && (status?.updateAvailable == true
        || (status?.running == true && status?.compatible == false))
  }
  var canUpdate: Bool {
    manageable && status?.blocked != true && !busy && !checking && status?.availableBuild != nil
      && needsUpdate
  }
  var canRestart: Bool { manageable && status?.blocked != true && !busy }
}
