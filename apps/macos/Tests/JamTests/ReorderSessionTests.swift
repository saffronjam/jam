import CoreGraphics

struct ReorderSessionTests {
  static func run() throws {
    // Expanded project, compact workspace, and another compact workspace.
    // Repeated drag events in a newly animated row must not reverse the move.
    var thin = ReorderSession(centers: [50, 126, 160], source: 1, grabPosition: 126)
    assert(thin.destination(at: 75) == 1)
    assert(thin.destination(at: 46) == 0)
    for _ in 0..<100 { assert(thin.destination(at: 46) == 0) }
    for y in [49.0, 51, 52, 48] { assert(thin.destination(at: y) == 0) }
    assert(thin.destination(at: 54) == 1)
    assert(thin.destination(at: 164) == 2)
    for _ in 0..<100 { assert(thin.destination(at: 164) == 2) }
    assert(thin.destination(at: 159) == 2)
    assert(thin.destination(at: 156) == 1)

    // Grab near the top of an expanded project: its center follows the drag
    // with that offset, and it moves as a single group past smaller siblings.
    var group = ReorderSession(centers: [50, 126, 160], source: 0, grabPosition: 20)
    assert(group.destination(at: 20) == 0)
    assert(group.destination(at: 97) == 0)
    assert(group.destination(at: 101) == 1)
    assert(group.destination(at: 140) == 2)
    assert(group.destination(at: 20) == 0)

    var only = ReorderSession(centers: [20], source: 0, grabPosition: 10)
    assert(only.destination(at: -100) == 0)
    assert(only.destination(at: 1000) == 0)
    // A short tab dragged past a wide tab must keep its insertion slot even
    // though that wide tab animates left underneath the stationary pointer.
    var tab = ReorderSession(centers: [46, 205, 376], source: 0, grabPosition: 13)
    assert(tab.destination(at: 176) == 1)
    for _ in 0..<100 { assert(tab.destination(at: 176) == 1) }
    for x in [172.0, 171, 174, 173] { assert(tab.destination(at: x) == 1) }
    assert(tab.destination(at: 168) == 0)
    assert(tab.destination(at: 400) == 2)
    assert(tab.destination(at: 13) == 0)

    // Starting halfway offscreen and scrolling the strip preserves document
    // coordinates; the grab point need not be at the middle of the tab.
    var scrolled = ReorderSession(centers: [46, 205, 376], source: 1, grabPosition: 110)
    assert(scrolled.destination(at: 110) == 1)
    assert(scrolled.destination(at: 285) == 2)
    for _ in 0..<100 { assert(scrolled.destination(at: 285) == 2) }
    assert(scrolled.destination(at: -55) == 0)
    print(
      "Reordering checks passed: unequal row heights and tab widths, stable hover, reversal, jitter, scrolling coordinates, and grab offsets."
    )
  }
}
