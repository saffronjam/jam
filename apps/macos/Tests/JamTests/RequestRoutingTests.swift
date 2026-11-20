import Foundation

struct RequestRoutingTests {
  static func run() throws {
    let local: Set<String> = ["local-w", "local-tab", "local-pane"]
    let remoteClose = ["WorkspaceClose": ["workspace": "remote|w"]]
    assert(RequestRouting.endpoint(remoteClose, fallback: "", localIDs: local) == "remote")
    assert(
      RequestRouting.endpoint(
        ["Close": ["pane": "remote|coordinator"]], fallback: "", localIDs: local) == "remote")
    assert(
      RequestRouting.endpoint(
        ["WorkspaceClose": ["workspace": "local-w"]], fallback: "remote", localIDs: local
      ).isEmpty)
    assert(
      RequestRouting.endpoint(
        ["NewTab": ["workspace": "local-w"]], fallback: "remote", localIDs: local
      ).isEmpty)
    assert(RequestRouting.endpoint("State", fallback: "remote", localIDs: local) == "remote")
    assert(
      RequestRouting.endpoint(
        ["Coordination": ["provider": "codex"]], fallback: "remote", localIDs: local) == "remote")
    print(
      "Request routing passed: context actions use target identity before guards and reconciliation."
    )
  }
}
