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

  func neighbor(of id: String) -> String? {
    guard let source = panes.first(where: { $0.id == id })?.frame else { return nil }
    func rank(_ target: CGRect) -> (Int, CGFloat, CGFloat) {
      let sharedWidth = min(source.maxX, target.maxX) - max(source.minX, target.minX)
      let sharedHeight = min(source.maxY, target.maxY) - max(source.minY, target.minY)
      let gap = SplitLayout.divider + 0.001
      let sharesEdge =
        (sharedWidth > 0 && sharedHeight >= -gap) || (sharedHeight > 0 && sharedWidth >= -gap)
      let dx = max(0, target.minX - source.midX, source.midX - target.maxX)
      let dy = max(0, target.minY - source.midY, source.midY - target.maxY)
      let centerX = target.midX - source.midX
      let centerY = target.midY - source.midY
      return (sharesEdge ? 0 : 1, dx * dx + dy * dy, centerX * centerX + centerY * centerY)
    }
    return panes.lazy.filter { $0.id != id }.min { rank($0.frame) < rank($1.frame) }?.id
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
