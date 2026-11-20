import Foundation

enum SidebarGeometry {
  static let machineLeading: CGFloat = 16
  static let iconSize: CGFloat = 14
  static let statusSize: CGFloat = 6
  static let statusLeading = machineLeading + (iconSize - statusSize) / 2
  static let workspaceHighlightLeading = statusLeading + statusSize + 6
  static let workspaceContentInset: CGFloat = 18
  static let worktreeIconInset: CGFloat = 6
  static let selectionPadding: CGFloat = 8

  static func rowHeight(hasBranch: Bool, isMachine: Bool) -> CGFloat {
    (hasBranch ? 46 : 32) + (isMachine ? 0 : 2)
  }
}

enum RemoteSidebarState {
  case loading, ready, unavailable

  static func resolve(loaded: Bool, loading: Bool, paused: Bool, hasResult: Bool) -> Self {
    if loaded { return .ready }
    if paused { return .unavailable }
    return loading || !hasResult ? .loading : .unavailable
  }
}

enum RemoteConnectionState: Equatable {
  case connected, connecting, reconnecting, retryWaiting, waiting, disconnected

  static func resolve(
    connected: Bool, attempting: Bool, paused: Bool, hasConnected: Bool, hasResult: Bool
  ) -> Self {
    if paused { return .disconnected }
    if attempting { return hasConnected || connected ? .reconnecting : .connecting }
    if connected { return .connected }
    return hasResult ? .retryWaiting : .waiting
  }
  var isAttempting: Bool { self == .connecting || self == .reconnecting }
  var label: String? {
    switch self {
    case .connected: return nil
    case .connecting: return "Connecting…"
    case .reconnecting: return "Reconnecting…"
    case .retryWaiting: return "Retrying soon"
    case .waiting: return "Waiting…"
    case .disconnected: return "Offline"
    }
  }
}

/// Ordered edits keep existing outline items alive as other rows appear around them.
enum SidebarEdit: Equatable {
  case remove(Int)
  case insert(Int)
  case move(Int, Int)

  static func between(_ old: [String], _ new: [String]) -> [Self] {
    var current = old
    let wanted = Set(new)
    var edits: [Self] = []
    for index in current.indices.reversed() where !wanted.contains(current[index]) {
      current.remove(at: index)
      edits.append(.remove(index))
    }
    for (index, key) in new.enumerated() {
      if let previous = current.firstIndex(of: key) {
        if previous != index {
          current.insert(current.remove(at: previous), at: index)
          edits.append(.move(previous, index))
        }
      } else {
        current.insert(key, at: index)
        edits.append(.insert(index))
      }
    }
    return edits
  }
}
