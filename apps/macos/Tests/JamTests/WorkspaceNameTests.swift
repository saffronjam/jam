import Foundation

struct WorkspaceNameTests {
  static func run() throws {
    assert(
      WorkspaceName.display("edge", automatic: true, directory: "/home/edge", home: "/home/edge")
        == "~")
    assert(
      WorkspaceName.display(
        "developer", automatic: true, directory: "/Users/developer/", home: "/Users/developer")
        == "~")
    assert(
      WorkspaceName.display(
        "edge", automatic: true, directory: "file://dev-server/home/edge", home: "/home/edge")
        == "~")
    assert(
      WorkspaceName.display(
        "project", automatic: true, directory: "/home/edge/project", home: "/home/edge")
        == "project")
    assert(
      WorkspaceName.display(
        "My server", automatic: false, directory: "/home/edge", home: "/home/edge") == "My server")
    assert(
      WorkspaceName.display("edge", automatic: true, directory: "/home/edge", home: nil) == "edge")
    print("Workspace name tests passed: home aliases, remote paths, and manual overrides.")
  }
}
