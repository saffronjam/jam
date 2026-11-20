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
