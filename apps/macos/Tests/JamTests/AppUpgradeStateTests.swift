import Foundation

struct AppUpgradeStateTests {
  static func stageRoundTrip() throws {
    let sessions = ["pane-one": "generation-one", "pane-two": "generation-two"]
    let fixture: [String: Any] = [
      "staged": [
        "directory": "/tmp/verified-stage", "archive": "/tmp/verified-stage/Jam.zip",
        "release": [
          "schema": 1, "version": "0.0.3", "protocol": "jam/test", "min_macos": "14.0",
          "assets": [
            [
              "name": "Jam-v0.0.3-macos-arm64.zip", "platform": "macos", "arch": "aarch64",
              "sha256": String(repeating: "a", count: 64), "size": 42,
            ]
          ],
        ],
      ],
      "plan": [
        "installation": "/Applications/Jam.app", "local_running": true,
        "targets": [
          [
            "endpoint": "", "name": "Local", "panes": ["pane-one", "pane-two"],
            "sessions": sessions, "incarnation": "local-incarnation", "build": "local-build",
            "version": "0.0.1",
          ],
          [
            "endpoint": "remote", "name": "Build server", "panes": [] as [String],
            "sessions": [:] as [String: String], "incarnation": "remote-incarnation",
            "build": "remote-build", "version": NSNull(),
          ],
        ],
        "offline": ["Offline server"], "app_pids": [12345, 23456],
      ],
    ]
    let data = try JSONSerialization.data(withJSONObject: fixture)
    let decoded = try JSONDecoder().decode(AppUpgradeStage.self, from: data)
    assert(decoded.plan.targets[0].sessions == sessions)
    assert(decoded.plan.targets[1].sessions.isEmpty && decoded.plan.targets[1].version == nil)
    let encoded =
      try JSONSerialization.jsonObject(with: JSONEncoder().encode(decoded)) as! [String: Any]
    assert(
      NSDictionary(dictionary: fixture).isEqual(to: encoded),
      "Stage and plan must preserve every core field, including session generations and a null version"
    )
    let encodedPlan =
      try JSONSerialization.jsonObject(with: JSONEncoder().encode(decoded.plan)) as! [String: Any]
    assert(NSDictionary(dictionary: fixture["plan"] as! [String: Any]).isEqual(to: encodedPlan))
    print(
      "Upgrade plan round trip passed: live session generations, empty target maps, nullable versions, assets, offline machines, and app PIDs."
    )
  }

  static func run() throws {
    try stageRoundTrip()
    for phase in ["recovered", "rolled_back", "failed", "complete_with_pending"] {
      let status = try JSONSerialization.data(withJSONObject: [
        "phase": phase, "error": "Diagnostic",
      ])
      let validated = try AppUpgradeResponse.validate(status, action: "status")
      assert(validated == status)
    }
    do {
      let failure = try JSONSerialization.data(withJSONObject: ["error": "Unreadable journal"])
      _ = try AppUpgradeResponse.validate(failure, action: "status")
      assertionFailure("Native error envelopes must still fail")
    } catch { assert(error.localizedDescription == "Unreadable journal") }
    assert(AppUpgradeRequirement.satisfies("0.0.10", minimum: "0.0.2"))
    assert(!AppUpgradeRequirement.satisfies("0.0.2", minimum: "0.0.10"))
    assert(AppUpgradeRequirement.satisfies("0.0.3", minimum: "0.0.3-beta.1"))
    assert(!AppUpgradeRequirement.satisfies("0.0.3", minimum: "0.0.4"))
    assert(!AppUpgradeRequirement.satisfies("0.0.3", minimum: "malformed"))
    var requirements = AppUpgradeRequirements()
    assert(requirements.observe(endpoint: "build", minimum: "0.0.3", manual: false))
    assert(
      !requirements.observe(endpoint: "build", minimum: "0.0.3", manual: false),
      "Automatic retries must not repeat notifications")
    assert(
      requirements.observe(endpoint: "build", minimum: "0.0.3", manual: true),
      "Explicit reconnect can expose a skipped update")
    assert(requirements.observe(endpoint: "other", minimum: "0.0.10", manual: false))
    assert(requirements.minimum == "0.0.10")
    assert(requirements.observe(endpoint: "build", minimum: "0.0.4", manual: false))
    assert(
      requirements.minimum == "0.0.10",
      "A lower requirement must not hide a newer machine's requirement")
    requirements.forget(endpoint: "other")
    assert(requirements.minimum == "0.0.4", "Removing an endpoint must relax its obsolete minimum")
    requirements.retain(endpoints: [])
    assert(requirements.minimum == nil)
    let release = AppRelease(
      schema: 1, version: "0.0.3", protocol: "jam/test", minMacOS: "14.0", assets: [])
    var state = AppUpgradeSchedule()
    assert(!state.isDue(at: 0, automatic: false))
    assert(state.isDue(at: 0, automatic: true))
    state.succeeded(AppUpgradeCheck(current: "0.0.2", available: true, release: release), at: 100)
    assert(!state.isDue(at: 86_499, automatic: true))
    assert(state.isDue(at: 86_500, automatic: true))
    assert(state.visibleRelease(manual: false) == release)
    assert(state.visibleRelease(manual: true, current: "0.0.3") == nil)
    assert(state.visibleRelease(manual: false, current: "0.0.4") == nil)
    assert(state.visibleRelease(manual: false, current: "0.0.2") == release)
    state.skip("0.0.3")
    assert(state.visibleRelease(manual: false) == nil)
    assert(state.visibleRelease(manual: true) == release, "Manual checks expose skipped releases")
    let persisted = try JSONDecoder().decode(
      AppUpgradeSchedule.self, from: JSONEncoder().encode(state))
    assert(persisted.skippedVersion == "0.0.3" && persisted.visibleRelease(manual: false) == nil)
    let older = AppRelease(
      schema: 1, version: "0.0.2", protocol: "jam/test", minMacOS: "14.0", assets: [])
    state.succeeded(AppUpgradeCheck(current: "0.0.1", available: true, release: older), at: 150)
    assert(
      state.visibleRelease(manual: false) == nil,
      "An older latest-release response must not bypass a newer skip")
    assert(state.visibleRelease(manual: true) == older)
    let newer = AppRelease(
      schema: 1, version: "0.0.4", protocol: "jam/test", minMacOS: "14.0", assets: [])
    state.succeeded(AppUpgradeCheck(current: "0.0.2", available: true, release: newer), at: 200)
    assert(state.visibleRelease(manual: false) == newer)
    state.failed(at: 300)
    assert(!state.isDue(at: 1_199, automatic: true) && state.isDue(at: 1_200, automatic: true))
    assert(
      state.visibleRelease(manual: false) == newer,
      "Failure retains the last authenticated candidate")
    state.failed(at: 400)
    assert(state.nextCheck == 2_200)
    for _ in 0..<20 { state.failed(at: 500) }
    assert(state.nextCheck <= 500 + 6 * 60 * 60)
    state.succeeded(AppUpgradeCheck(current: "0.0.4", available: false, release: nil), at: 600)
    assert(state.failures == 0 && state.release == nil && state.lastSuccess == 600)
    print(
      "App upgrade scheduling passed: daily checks, bounded retries, cached candidates, exact-version skips, manual override, and persisted state."
    )
  }
}
