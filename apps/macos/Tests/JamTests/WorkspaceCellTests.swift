import AppKit

struct WorkspaceCellTests {
  static func run() throws {
    _ = NSApplication.shared
    let github = ReviewRequest(
      provider: "github", number: 123, url: "https://github.com/o/r/pull/123", title: "Feature",
      draft: false)
    let gitlab = ReviewRequest(
      provider: "gitlab", number: 456, url: "https://gitlab.com/o/r/-/merge_requests/456",
      title: "Draft feature", draft: true)
    assert(github.label == "#123" && gitlab.label == "!456")
    assert(gitlab.tooltip.contains("draft"))
    assert(
      ReviewRequest(provider: "github", number: 1, url: "file:///tmp/file", title: "", draft: false)
        .link == nil)
    for width in [80.0, 160, 280] {
      for branch in [nil, Optional(String(repeating: "long-branch-", count: 20))] {
        for review in [nil, github, gitlab] {
          for changed in [true, false] {
            let cell = WorkspaceCell(
              name: String(repeating: "long-name-", count: 20), branch: branch, tint: nil,
              symbol: "folder",
              diff: GitSummary(
                added: changed ? 12345 : 0, removed: changed ? 987 : 0, review: review))
            cell.frame = NSRect(x: 0, y: 0, width: width, height: 46)
            cell.layout()
            assert(cell.nameField.lineBreakMode == .byTruncatingTail)
            assert(cell.titleRow.frame.maxX <= cell.bounds.maxX)
            for view in cell.titleRow.subviews where !view.isHidden {
              assert(view.frame.minX >= 0 && view.frame.maxX <= cell.titleRow.bounds.width)
            }
            for label in cell.subviews.compactMap({ $0 as? NSTextField }) {
              assert(label.frame.maxX <= cell.bounds.maxX)
              assert(label.lineBreakMode == .byTruncatingTail)
              assert(label.maximumNumberOfLines == 1)
            }
            let button = cell.titleRow.subviews.compactMap { $0 as? NSButton }.first
            assert((button != nil) == (review != nil))
            if let button, !button.isHidden {
              assert(button.title == review?.label)
              assert(button.toolTip == review?.tooltip)
              let added = cell.titleRow.subviews.compactMap { $0 as? NSTextField }.first {
                $0.stringValue.hasPrefix("+")
              }
              if let added, !added.isHidden { assert(button.frame.maxX <= added.frame.minX) }
            }
          }
        }
      }
    }
    print(
      "Workspace label checks passed: long names, long branches, counters, and narrow rows stay within bounds."
    )
  }
}
