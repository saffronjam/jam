import Foundation

struct TerminalCachePolicyTests {
  static func run() throws {
    var cache = TerminalCachePolicy()
    for id in ["old", "closed", "recent", "active"] { cache.touch(id) }
    let live: Set<String> = ["old", "recent", "active"]
    assert(cache.retain(live: live, visible: ["active"], capacity: 2) == ["recent", "active"])
    // Switching back reuses the existing surface and protects it from eviction.
    cache.touch("recent")
    cache.touch("new")
    assert(
      cache.retain(live: live.union(["new"]), visible: ["recent"], capacity: 2) == [
        "recent", "new",
      ])
    // All visible panes survive even when a large split layout exceeds the cache limit.
    cache.touch("active")
    assert(
      cache.retain(live: live.union(["new"]), visible: ["recent", "new", "active"], capacity: 1)
        .count == 3)
    assert(cache.retain(live: [], visible: [], capacity: 16).isEmpty)
    assert(cache.recent.isEmpty)
    print("Terminal cache tests passed: reuse, recency, closed panes, and visible-pane protection.")
  }
}
