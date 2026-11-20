import Foundation

struct WorktreeChoicesTests {
  static func run() throws {
    let checkouts = [
      WorktreeCheckout(path: "/repo", branch: "main", main: true, locked: false),
      WorktreeCheckout(path: "/wt/API-fix", branch: "api-fix", main: false, locked: false),
      WorktreeCheckout(path: "/wt/ui", branch: "ui", main: false, locked: true),
    ]
    let choices = WorktreeChoices(
      checkouts: checkouts, query: "", opened: [WorktreeChoices.normalized("/repo/")])
    assert(choices.filtered.count == 3)
    assert(choices.selection("/repo") == "/wt/API-fix")
    assert(choices.move("/wt/API-fix", by: 1) == "/wt/ui")
    assert(choices.move("/wt/ui", by: -1) == "/wt/API-fix")
    assert(choices.move("/wt/ui", by: 1) == "/wt/ui")
    let filtered = WorktreeChoices(checkouts: checkouts, query: "API /WT", opened: [])
    assert(filtered.filtered.map(\.path) == ["/wt/API-fix"])
    assert(filtered.selection("/wt/ui") == "/wt/API-fix")
    let opened = WorktreeChoices(
      checkouts: checkouts, query: "",
      opened: Set(checkouts.map { WorktreeChoices.normalized($0.path) }))
    assert(
      opened.available.isEmpty && opened.selection(nil) == nil && opened.move(nil, by: 1) == nil)
    let empty = WorktreeChoices(checkouts: checkouts, query: "missing", opened: [])
    assert(empty.filtered.isEmpty && empty.selection("/repo") == nil)
    print(
      "Worktree picker checks passed: open rows, filtering, selection, navigation, and empty results."
    )
  }
}
