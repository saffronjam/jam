import Foundation

struct PaneLiftTests {
  static func run() throws {
    let root = Node.split(
      false, 0.4, .leaf("left"), .split(true, 0.5, .leaf("top"), .leaf("bottom")))
    assert(root.removing("top")?.ids == ["left", "bottom"])
    assert(root.removing("left")?.ids == ["top", "bottom"])
    assert(root.removing("absent")?.ids == root.ids)
    assert(root.removing(nil)?.ids == root.ids)
    assert(Node.leaf("only").removing("only") == nil)
    print(
      "Pane lift checks passed: collapse source split, preserve sibling order, empty tab, and cancellation baseline."
    )
  }
}
