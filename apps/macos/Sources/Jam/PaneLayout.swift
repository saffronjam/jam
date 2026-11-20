import CoreGraphics
import Foundation

/// Layout is recursive; views are not. Every pane stays a sibling keyed by its
/// stable ID, so splitting or pruning a branch cannot crossfade surviving panes.
struct PaneLayout {
  struct PaneFrame: Identifiable {
    let id: String
    let frame: CGRect
  }
  struct DividerFrame: Identifiable {
    let id: String
    let path: [Bool]
    let vertical: Bool
    let frame: CGRect
    let parent: CGRect
  }
  var panes: [PaneFrame] = []
  var dividers: [DividerFrame] = []

  init(node: Node, size: CGSize, ratios: [String: Double] = [:]) {
    append(node, in: CGRect(origin: .zero, size: size), path: [], ratios: ratios)
  }
  private mutating func append(
    _ node: Node, in rect: CGRect, path: [Bool], ratios: [String: Double]
  ) {
    switch node {
    case .empty: break
    case .leaf(let id): panes.append(PaneFrame(id: id, frame: rect))
    case .split(let vertical, let ratio, let a, let b):
      let id = path.isEmpty ? "root" : path.map { $0 ? "b" : "a" }.joined()
      let total = vertical ? rect.height : rect.width
      let gap = min(SplitLayout.divider, max(0, total))
      let first = SplitLayout.first(
        total: total, ratio: ratios[id] ?? ratio,
        minimumA: vertical ? a.minimumSize.height : a.minimumSize.width,
        minimumB: vertical ? b.minimumSize.height : b.minimumSize.width)
      let second = max(0, total - gap - first)
      let firstRect = CGRect(
        x: rect.minX, y: rect.minY, width: vertical ? rect.width : first,
        height: vertical ? first : rect.height)
      let secondRect = CGRect(
        x: rect.minX + (vertical ? 0 : first + gap), y: rect.minY + (vertical ? first + gap : 0),
        width: vertical ? rect.width : second, height: vertical ? second : rect.height)
      let handle = CGRect(
        x: rect.minX + (vertical ? 0 : first + gap / 2 - 4),
        y: rect.minY + (vertical ? first + gap / 2 - 4 : 0), width: vertical ? rect.width : 8,
        height: vertical ? 8 : rect.height)
      dividers.append(
        DividerFrame(id: id, path: path, vertical: vertical, frame: handle, parent: rect))
      append(a, in: firstRect, path: path + [false], ratios: ratios)
      append(b, in: secondRect, path: path + [true], ratios: ratios)
    }
  }
}
