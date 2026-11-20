import Foundation

/// A bounded working set, with visible panes protected from eviction.
struct TerminalCachePolicy {
  private(set) var recent: [String] = []

  mutating func touch(_ pane: String) {
    recent.removeAll { $0 == pane }
    recent.insert(pane, at: 0)
  }

  mutating func retain(live: Set<String>, visible: Set<String>, capacity: Int) -> Set<String> {
    recent.removeAll { !live.contains($0) }
    for pane in recent.filter({ visible.contains($0) }) { touch(pane) }
    var kept = visible.intersection(Set(recent))
    let limit = max(capacity, kept.count)
    for pane in recent where kept.count < limit { kept.insert(pane) }
    recent.removeAll { !kept.contains($0) }
    return kept
  }
}
