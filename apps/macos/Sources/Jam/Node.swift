import Foundation

indirect enum Node {
  case empty
  case leaf(String)
  case split(Bool, Double, Node, Node)
  var ids: [String] {
    switch self {
    case .empty: return []
    case .leaf(let id): return [id]
    case .split(_, _, let a, let b): return a.ids + b.ids
    }
  }
  var minimumSize: CGSize {
    switch self {
    case .empty: return .zero
    case .leaf: return SplitLayout.leaf
    case .split(let vertical, _, let a, let b):
      return SplitLayout.minimum(a.minimumSize, b.minimumSize, vertical: vertical)
    }
  }
  init?(_ raw: Any) {
    if let name = raw as? String, name == "Empty" {
      self = .empty
      return
    }
    guard let d = raw as? [String: Any] else { return nil }
    if let id = d["Leaf"] as? String {
      self = .leaf(id)
    } else if let s = d["Split"] as? [String: Any], let a = Node(s["a"] as Any),
      let b = Node(s["b"] as Any)
    {
      self = .split(s["vertical"] as? Bool ?? false, s["ratio"] as? Double ?? 0.5, a, b)
    } else {
      return nil
    }
  }
}

extension Node {
  func splitting(_ pane: String, with newPane: String, vertical: Bool) -> Node {
    inserting(newPane, nextTo: pane, vertical: vertical, before: false)
  }

  func inserting(_ newPane: String, nextTo pane: String, vertical: Bool, before: Bool) -> Node {
    switch self {
    case .leaf(let id) where id == pane:
      let new = Node.leaf(newPane)
      return before ? .split(vertical, 0.5, new, self) : .split(vertical, 0.5, self, new)
    case .split(let direction, let ratio, let a, let b):
      return .split(
        direction, ratio, a.inserting(newPane, nextTo: pane, vertical: vertical, before: before),
        b.inserting(newPane, nextTo: pane, vertical: vertical, before: before))
    default: return self
    }
  }

  func splitSibling(of pane: String, vertical: Bool) -> String? {
    guard case .split(let direction, _, let a, let b) = self else { return nil }
    if direction == vertical, case .leaf(pane) = a, case .leaf(let sibling) = b {
      return sibling
    }
    return a.splitSibling(of: pane, vertical: vertical)
      ?? b.splitSibling(of: pane, vertical: vertical)
  }

  func removing(_ pane: String?) -> Node? {
    guard let pane else { return self }
    switch self {
    case .empty: return self
    case .leaf(let id): return id == pane ? nil : self
    case .split(let vertical, let ratio, let a, let b):
      switch (a.removing(pane), b.removing(pane)) {
      case (.some(let a), .some(let b)): return .split(vertical, ratio, a, b)
      case (.some(let a), .none): return a
      case (.none, .some(let b)): return b
      case (.none, .none): return nil
      }
    }
  }
}

struct PendingPaneSplit {
  let tab: String
  let source: String
  let vertical: Bool
  let previousPanes: [String]
  let placeholder = "pending-split-" + UUID().uuidString

  func preview(in root: Node) -> Node {
    guard root.ids == previousPanes else { return root }
    return root.splitting(source, with: placeholder, vertical: vertical)
  }

  func createdPane(in root: Node) -> String? {
    guard let pane = root.splitSibling(of: source, vertical: vertical),
      !previousPanes.contains(pane)
    else { return nil }
    return pane
  }
}

extension Node {
  func resizing(_ path: ArraySlice<Bool>, ratio: Double) -> Node? {
    guard case .split(let vertical, let previous, let left, let right) = self else { return nil }
    switch path.first {
    case nil: return .split(vertical, ratio, left, right)
    case false?:
      return left.resizing(path.dropFirst(), ratio: ratio).map {
        .split(vertical, previous, $0, right)
      }
    case true?:
      return right.resizing(path.dropFirst(), ratio: ratio).map {
        .split(vertical, previous, left, $0)
      }
    }
  }
}
