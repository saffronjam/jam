import Foundation

enum WorkspaceCreationAction: Equatable {
  case new
  case open(String)
  case createWorktree(source: String, branch: String, base: String, path: String?)
  case openWorktree(source: String, path: String)

  var opensExisting: Bool {
    switch self {
    case .open, .openWorktree: return true
    case .new, .createWorktree: return false
    }
  }
  func request(home: String?) -> [String: Any]? {
    switch self {
    case .new:
      guard let home else { return nil }
      return ["NewWorkspace": ["path": home]]
    case .open(let path): return ["CreateWorkspace": ["path": path, "name": ""]]
    case .openWorktree(let source, let path):
      return ["WorktreeOpen": ["path": path, "workspace": source]]
    case .createWorktree(let source, let branch, let base, let path):
      return [
        "WorktreeCreate": [
          "workspace": source, "branch": branch, "base": base,
          "path": path as Any? ?? NSNull(),
        ]
      ]
    }
  }
}

struct WorkspaceCreation {
  let started = ProcessInfo.processInfo.systemUptime
  let id: String
  let endpoint: String
  let previous: String
  let action: WorkspaceCreationAction
  let name: String
  let path: String
  var parent: String?
  var branch: String?
  var repository: String?
  var home: String?
  var incarnation: String?
  var token = UUID().uuidString.lowercased()
  var operation: String?
  var result: String?
  var failure: String?
  var canResubmit = false

  var proposedID: String { (endpoint.isEmpty ? "" : endpoint + "|") + token }
  init(
    endpoint: String, previous: String, action: WorkspaceCreationAction, name: String, path: String
  ) {
    self.endpoint = endpoint
    self.previous = previous
    self.action = action
    self.name = name
    self.path = path
    id = (endpoint.isEmpty ? "" : endpoint + "|") + "pending-" + token
  }
}
