import Foundation

struct AgentUsageTests {
  static func report(
    _ provider: String, account: String?, profile: String, active: Int = 1, label: String? = nil
  ) throws
    -> AgentUsageReport
  {
    let value: [String: Any] = [
      "provider": provider, "profile_key": profile, "account_key": account ?? NSNull(),
      "account_label": (label ?? (account == nil ? nil : "test@example.invalid")) as Any?
        ?? NSNull(), "active_sessions": active, "checking": false,
      "checked_at": 100, "last_success_at": 100,
      "windows": [
        ["key": "five_hour", "label": "5h", "percent": 25],
        ["key": "weekly", "label": "7d", "percent": 50],
        ["key": "weekly:model", "label": "7d Model", "percent": 10],
      ],
    ]
    return try JSONDecoder().decode(
      AgentUsageReport.self, from: JSONSerialization.data(withJSONObject: value))
  }
  static func run() throws {
    let local = try report("codex", account: "account-a", profile: "local")
    let shared = try report("codex", account: "account-a", profile: "remote", active: 2)
    let other = try report("codex", account: "account-b", profile: "remote")
    let machines = [("", "Local"), ("remote", "Remote")]
    let merged = AgentUsagePage.build(
      reports: ["": [local], "remote": [shared]], machines: machines)
    assert(merged.count == 1 && merged[0].label == "test@example.invalid")
    assert(merged[0].reports[0].activeSessions == 3)
    assert(merged[0].reports[0].primaryWindows.count == 2)
    assert(merged[0].reports[0].windows.count == 3)
    assert(
      AgentUsagePage.build(reports: ["": [local], "remote": [other]], machines: machines).count == 2
    )
    let unknown = try report("claude", account: nil, profile: "unknown")
    assert(
      AgentUsagePage.build(reports: ["": [unknown], "remote": [unknown]], machines: machines).count
        == 2)
    let claude = try report("claude", account: "claude-a", profile: "claude")
    let subset = AgentUsagePage.build(
      reports: ["": [claude], "remote": [shared, claude]], machines: machines)
    assert(subset.count == 1 && subset[0].reports.map(\.provider) == ["codex", "claude"])
    assert(subset[0].reports[1].activeSessions == 2)
    let emailOnly = try report(
      "claude", account: nil, profile: "default", label: " TEST@example.invalid ")
    let byEmail = AgentUsagePage.build(
      reports: ["": [emailOnly], "remote": [shared, claude]], machines: machines)
    assert(byEmail.count == 1 && byEmail[0].reports.count == 2)
    assert(byEmail[0].reports.last?.activeSessions == 2)
    let personal = try report(
      "claude", account: "personal-id", profile: "personal", label: "personal@example.invalid")
    let distinct = AgentUsagePage.build(
      reports: ["": [local, personal], "remote": [shared, claude]], machines: machines)
    assert(distinct.count == 2 && distinct[0].reports.count == 1 && distinct[1].reports.count == 2)
    let available = AgentUsagePage.build(
      reports: ["": [local], "remote": [shared]], machines: machines, unavailable: [""])
    assert(available[0].sources[local.id] == "remote")
    let repeated = AgentUsagePage.build(reports: ["": [local, shared]], machines: machines)
    assert(
      repeated.count == 1 && repeated[0].reports.count == 1
        && repeated[0].reports[0].activeSessions == 3)
    let inactive = try report("claude", account: "gone", profile: "gone", active: 0)
    assert(AgentUsagePage.build(reports: ["": [inactive]], machines: machines).count == 1)
    assert(AgentUsagePage.build(reports: [:], machines: machines).isEmpty)
    func unavailable(_ error: String, checking: Bool = false) throws -> AgentUsageReport {
      try JSONDecoder().decode(
        AgentUsageReport.self,
        from: JSONSerialization.data(withJSONObject: [
          "provider": "claude", "profile_key": "blocked", "active_sessions": 1,
          "windows": [], "checking": checking, "error": error,
        ]))
    }
    let blocked = try unavailable("Claude login is missing or Keychain access is unavailable")
    assert(blocked.needsKeychainAccess && blocked.availabilityText == "Keychain access needed")

    let service = try unavailable("Usage service is temporarily unavailable")
    assert(!service.needsKeychainAccess && service.availabilityText == service.error)
    let checking = try unavailable("Keychain unavailable", checking: true)
    assert(checking.availabilityText == "Reading usage…")
    print(
      "Agent usage checks passed: account grouping, separate unknown identities, dynamic windows, and inactive providers."
    )
  }
}
