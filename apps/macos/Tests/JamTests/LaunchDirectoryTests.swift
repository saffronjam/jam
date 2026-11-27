import Foundation

struct LaunchDirectoryTests {
  static func run() throws {
    let base = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let cwd = base.appendingPathComponent("working folder ü")
    let home = base.appendingPathComponent("home")
    try FileManager.default.createDirectory(at: cwd, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: base) }
    let working = cwd.resolvingSymlinksInPath()
    let userHome = home.resolvingSymlinksInPath()
    func resolve(_ arguments: [String] = ["Jam"], terminal: Bool = false) throws -> URL? {
      try LaunchDirectory.resolve(
        arguments: arguments, cwd: working, home: userHome, fromTerminal: terminal)
    }
    func reject(_ arguments: [String]) {
      do {
        _ = try resolve(arguments)
        fatalError("Expected invalid launch arguments to fail: \(arguments)")
      } catch {}
    }
    func check(_ value: Bool, _ message: String) { precondition(value, message) }
    let alias = base.appendingPathComponent("linked working folder")
    try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: working)
    check(
      LaunchDirectory.matches(alias.path, directory: working),
      "Workspace creation replies must match the requested directory across filesystem aliases")
    check(
      LaunchDirectory.matches(
        working.path, directory: URL(fileURLWithPath: working.path, isDirectory: true)),
      "Directory URL trailing slashes must not change path identity")
    check(
      !LaunchDirectory.matches(userHome.path, directory: working),
      "A different workspace must never match the launch directory")
    check(try resolve() == userHome, "Desktop launch must use home")
    check(try resolve(terminal: true) == working, "Terminal launch must use cwd")
    check(
      try resolve(["Jam", "--restore-session"]) == nil,
      "Updater relaunch must restore the existing session without opening home")
    check(
      try resolve(["Jam", "--restore-session", "--directory", working.path]) == working,
      "An explicitly requested directory takes precedence over session restoration")
    check(
      try resolve(["Jam", "--directory", working.path]) == working,
      "Explicit paths must survive Launch Services")
    check(
      try resolve(["Jam", "--directory", "../home"], terminal: true) == userHome,
      "Relative paths must use cwd")
    check(
      try resolve(["Jam", "--directory", "~"], terminal: true) == userHome, "Tilde must use home")
    reject(["Jam", "--directory"])
    reject(["Jam", "--restore-session", "--directory"])
    reject(["Jam", "--directory", "missing"])
    print(
      "Launch directory checks passed: desktop, terminal, restore, explicit, relative, tilde, and invalid paths."
    )
  }
}
