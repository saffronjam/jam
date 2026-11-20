import Foundation

struct ServerStatus: Decodable {
  let running: Bool
  let compatible: Bool
  let build: String?
  let availableBuild: String?
  let updateAvailable: Bool
  let terminals: Int?
  let error: String?
  var readyForScheduledUpdate: Bool {
    running && compatible && availableBuild != nil && updateAvailable && terminals == 0
  }
  enum CodingKeys: String, CodingKey {
    case running, compatible, build, terminals, error
    case availableBuild = "available_build"
    case updateAvailable = "update_available"
  }
}
struct ServerAction: Identifiable {
  enum Kind { case update, restart }
  let endpoint: String
  let name: String
  let kind: Kind
  var id: String { endpoint + (kind == .update ? ":update" : ":restart") }
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
  private var manageable: Bool { connected || incompatibleServer }
  var canReconnect: Bool { !connected && !incompatibleServer && !connecting && !busy }
  var canDisconnect: Bool { (connected || connecting) && !busy }
  var canCheck: Bool { manageable && !busy && !checking }
  var needsUpdate: Bool {
    manageable
      && (status?.updateAvailable == true
        || (status?.running == true && status?.compatible == false))
  }
  var canUpdate: Bool {
    manageable && !busy && !checking && status?.availableBuild != nil && needsUpdate
  }
  var canRestart: Bool { manageable && !busy }
}
