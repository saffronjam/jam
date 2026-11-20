import Foundation

/// Machine selection remains meaningful even when that machine has no workspaces.
enum WorkspaceSelection {
  static func endpoint(of workspace: String) -> String {
    workspace.split(separator: "|", maxSplits: 1).count == 2
      ? String(workspace.split(separator: "|", maxSplits: 1)[0]) : ""
  }

  static func restoredMachine(workspace: String, machine: String) -> String {
    workspace.isEmpty ? machine : endpoint(of: workspace)
  }

  static func reconcile(
    selected: String, machine: String, refreshed: String, workspaces: [String]
  ) -> String {
    guard machine == refreshed, !workspaces.contains(selected) else { return selected }
    return workspaces.first { endpoint(of: $0) == machine } ?? ""
  }

  static func ordered<T>(_ workspaces: [T], machines: [String], id: (T) -> String) -> [T] {
    let ranks = Dictionary(
      machines.enumerated().map { ($0.element, $0.offset) }, uniquingKeysWith: min)
    return workspaces.enumerated().sorted { a, b in
      let first = ranks[endpoint(of: id(a.element))] ?? Int.max
      let second = ranks[endpoint(of: id(b.element))] ?? Int.max
      return first == second ? a.offset < b.offset : first < second
    }.map(\.element)
  }
}
