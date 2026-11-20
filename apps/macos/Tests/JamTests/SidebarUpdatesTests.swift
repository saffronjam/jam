import Foundation

struct SidebarUpdatesTests {
  static func run() throws {
    assert(
      RemoteConnectionState.resolve(
        connected: true, attempting: false, paused: false, hasConnected: true, hasResult: true)
        == .connected)
    assert(
      RemoteConnectionState.resolve(
        connected: false, attempting: true, paused: false, hasConnected: true, hasResult: true)
        == .reconnecting)
    assert(
      RemoteConnectionState.resolve(
        connected: false, attempting: false, paused: false, hasConnected: true, hasResult: true)
        == .retryWaiting)
    assert(
      RemoteConnectionState.resolve(
        connected: false, attempting: true, paused: false, hasConnected: false, hasResult: false)
        == .connecting)
    assert(
      RemoteConnectionState.resolve(
        connected: true, attempting: true, paused: true, hasConnected: true, hasResult: true)
        == .disconnected)
    assert(!RemoteConnectionState.connected.isAttempting)
    assert(!RemoteConnectionState.retryWaiting.isAttempting)
    assert(RemoteConnectionState.reconnecting.isAttempting)
    assert(
      RemoteSidebarState.resolve(loaded: false, loading: false, paused: false, hasResult: false)
        == .loading)
    assert(
      RemoteSidebarState.resolve(loaded: false, loading: false, paused: true, hasResult: false)
        == .unavailable)
    assert(
      RemoteSidebarState.resolve(loaded: false, loading: false, paused: false, hasResult: true)
        == .unavailable)
    assert(
      RemoteSidebarState.resolve(loaded: false, loading: true, paused: false, hasResult: true)
        == .loading)
    // Regular background polls and a lost connection must not hide previously loaded workspaces.
    assert(
      RemoteSidebarState.resolve(loaded: true, loading: true, paused: false, hasResult: true)
        == .ready)
    assert(
      RemoteSidebarState.resolve(loaded: true, loading: false, paused: true, hasResult: true)
        == .ready)

    let layouts = [
      [], ["local", "remote"], ["local", "a", "b", "remote"],
      ["remote", "b", "new", "a"], ["b", "a"], ["new"],
    ]
    for old in layouts {
      for new in layouts {
        var applied = old
        for edit in SidebarEdit.between(old, new) {
          switch edit {
          case .remove(let index): applied.remove(at: index)
          case .insert(let index): applied.insert(new[index], at: index)
          case .move(let from, let to): applied.insert(applied.remove(at: from), at: to)
          }
        }
        assert(applied == new)
        if old == new { assert(SidebarEdit.between(old, new).isEmpty) }
      }
    }
    // Discovering remote children leaves both machine header identities intact.
    assert(SidebarEdit.between(["local", "remote"], ["local", "remote"]).isEmpty)
    assert(SidebarEdit.between([], ["workspace"]) == [.insert(0)])
    print(
      "Sidebar updates passed: initial loading, retries, stable polls, and incremental row edits.")
  }
}
