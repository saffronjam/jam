import Foundation

/// LaunchServices does not preserve a terminal's working directory. Launchers
/// can pass --directory explicitly; direct terminal launches inherit cwd.
enum LaunchDirectory {
  static func matches(_ path: String, directory: URL) -> Bool {
    URL(fileURLWithPath: path).standardizedFileURL.resolvingSymlinksInPath().path
      == directory.standardizedFileURL.resolvingSymlinksInPath().path
  }

  static func resolve(arguments: [String], cwd: URL, home: URL, fromTerminal: Bool) throws -> URL? {
    // The updater reopens the existing session. LaunchServices otherwise
    // supplies no directory, which would create a new home workspace.
    // An explicitly requested directory still takes precedence.
    if arguments.contains("--restore-session") && !arguments.contains("--directory") {
      return nil
    }
    var result = fromTerminal ? cwd : home
    if let index = arguments.firstIndex(of: "--directory") {
      guard arguments.indices.contains(index + 1), !arguments[index + 1].isEmpty else {
        throw NSError(domain: "Pass a folder after --directory", code: 1)
      }
      let path = arguments[index + 1]
      if path == "~" {
        result = home
      } else if path.hasPrefix("~/") {
        result = home.appendingPathComponent(String(path.dropFirst(2)))
      } else if path.hasPrefix("/") {
        result = URL(fileURLWithPath: path)
      } else {
        result = cwd.appendingPathComponent(path)
      }
    }
    result = result.standardizedFileURL.resolvingSymlinksInPath()
    var directory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: result.path, isDirectory: &directory),
      directory.boolValue
    else {
      throw NSError(domain: "Launch directory does not exist: \(result.path)", code: 2)
    }
    return result
  }
}
