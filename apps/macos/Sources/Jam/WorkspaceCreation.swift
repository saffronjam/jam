import Foundation

/// UI-only identity while the daemon creates a workspace. Never sent to the daemon.
struct WorkspaceCreation {
  let id: String
  let endpoint: String
  let home: String?
  let previous: String
  let knownWorkspaceIDs: Set<String>

  init(endpoint: String, home: String?, previous: String, knownWorkspaceIDs: Set<String> = []) {
    self.endpoint = endpoint
    self.home = home
    self.previous = previous
    self.knownWorkspaceIDs = knownWorkspaceIDs
    id = (endpoint.isEmpty ? "" : endpoint + "|") + "pending-" + UUID().uuidString
  }

  func includesDuringCreation(_ workspace: String) -> Bool {
    WorkspaceSelection.endpoint(of: workspace) != endpoint || knownWorkspaceIDs.contains(workspace)
  }

  func selection(after created: String?, selected: String, available: [String]) -> String {
    // Completion must not take focus from a workspace the user selected meanwhile.
    guard selected == id else { return selected }
    if let created { return created }
    if available.contains(previous) { return previous }
    return available.first { WorkspaceSelection.endpoint(of: $0) == endpoint } ?? ""
  }
}
