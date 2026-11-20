import Foundation

/// Resolve identity before choosing the revision guard, queue, or state owner.
enum RequestRouting {
  static func endpoint(_ value: Any, fallback: String, localIDs: Set<String>) -> String {
    let keys: Set<String> = [
      "id", "pane", "workspace", "tab", "Leaf", "active_pane", "agent", "owner", "task", "to",
    ]
    func find(_ value: Any) -> String? {
      if let object = value as? [String: Any] {
        for key in object.keys.sorted() {
          let child = object[key]!
          if keys.contains(key), let id = child as? String {
            if let divider = id.firstIndex(of: "|") { return String(id[..<divider]) }
            if localIDs.contains(id) { return "" }
          }
          if let endpoint = find(child) { return endpoint }
        }
      } else if let array = value as? [Any] {
        for child in array { if let endpoint = find(child) { return endpoint } }
      }
      return nil
    }
    return find(value) ?? fallback
  }
}
