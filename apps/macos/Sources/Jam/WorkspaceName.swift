import Foundation

enum WorkspaceName {
  static func display(_ name: String, automatic: Bool, directory: String, home: String?) -> String {
    guard automatic, let home, !home.isEmpty else { return name }
    func normalized(_ path: String) -> String {
      let path = path.hasPrefix("file://") ? (URL(string: path)?.path ?? path) : path
      return URL(fileURLWithPath: path).standardizedFileURL.path
    }
    return normalized(directory) == normalized(home) ? "~" : name
  }
}
