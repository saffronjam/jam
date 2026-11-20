import Foundation

extension Store {
  func refreshGitSummaries() {
    guard ready, canReadGitSummary, !gitSummaryLoading,
      Date().timeIntervalSince(gitSummaryTime) > 3
    else { return }
    gitSummaryLoading = true
    gitSummaryTime = Date()
    let paths = Dictionary(
      grouping: workspaces.filter {
        ($0.parent == nil ? $0.activeRepository : $0.repository) != nil
      },
      by: {
        WorkspaceSelection.endpoint(of: $0.id) + ":"
          + ($0.parent == nil ? ($0.context?.path ?? $0.path) : $0.path)
      })
    let observed = Dictionary(
      uniqueKeysWithValues: workspaces.map { ($0.id, $0.context?.path ?? $0.path) })
    DispatchQueue.global(qos: .utility).async {
      var summaries: [String: GitSummary] = [:]
      for group in paths.values {
        guard let workspace = group.first,
          let values = rpc(["GitSummary": ["workspace": workspace.id]])["GitSummary"]
            as? [String: Any],
          let added = values["added"] as? Int, let removed = values["removed"] as? Int
        else { continue }
        let review = (values["review"] as? [String: Any]).flatMap { value -> ReviewRequest? in
          guard let data = try? JSONSerialization.data(withJSONObject: value) else { return nil }
          return try? JSONDecoder().decode(ReviewRequest.self, from: data)
        }
        for workspace in group {
          summaries[workspace.id] = GitSummary(added: added, removed: removed, review: review)
        }
      }
      let result = summaries
      DispatchQueue.main.async {
        self.gitSummaryLoading = false
        let valid = result.filter { id, _ in
          self.workspaces.first(where: { $0.id == id }).map {
            ($0.context?.path ?? $0.path) == observed[id]
          } ?? false
        }
        if self.gitSummaries != valid { self.gitSummaries = valid }
      }
    }
  }
}
