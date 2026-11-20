import CoreGraphics
import Foundation

struct PaneLayoutTests {
  static func run() throws {
    let original = Node.split(false, 0.5, .leaf("left"), .leaf("right"))
    let split = Node.split(
      false, 0.5, .leaf("left"), .split(true, 0.5, .leaf("right"), .leaf("new")))
    let size = CGSize(width: 1000, height: 700)
    let before = PaneLayout(node: original, size: size)
    let after = PaneLayout(node: split, size: size)
    assert(before.panes.first!.id == after.panes.first!.id)
    assert(before.panes.first!.frame == after.panes.first!.frame, "Unchanged sibling must not move")
    let closed = PaneLayout(node: split.removing("right")!, size: size)
    assert(closed.panes.map(\.id) == ["left", "new"])
    assert(closed.panes.last!.frame == before.panes.last!.frame, "Survivor fills the closed split")
    let dragged = PaneLayout(node: original.removing("right")!, size: size)
    assert(dragged.panes[0].frame == CGRect(origin: .zero, size: size))
    for size in [CGSize(width: 1, height: 1), CGSize(width: 80, height: 60), size] {
      let layout = PaneLayout(node: split, size: size)
      assert(layout.panes.map(\.id) == ["left", "right", "new"])
      for pane in layout.panes {
        assert(pane.frame.minX >= 0 && pane.frame.minY >= 0)
        assert(pane.frame.maxX <= size.width + 0.001 && pane.frame.maxY <= size.height + 0.001)
      }
      for (index, a) in layout.panes.enumerated() {
        for b in layout.panes.dropFirst(index + 1) { assert(!a.frame.intersects(b.frame)) }
      }
    }
    let resized = PaneLayout(node: split, size: size, ratios: ["b": 0.7])
    assert(resized.panes[0].frame == after.panes[0].frame)
    assert(resized.panes[1].frame.height > after.panes[1].frame.height)
    print(
      "Flat pane layout checks passed: stable identities, unaffected siblings, split/close/lift frames, nested dividers, and bounded small windows."
    )
  }
}
