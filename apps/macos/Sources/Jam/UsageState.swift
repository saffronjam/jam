import Foundation

struct AgentUsageWindow: Decodable, Equatable, Identifiable {
  let key: String
  let label: String
  let percent: Double?
  let resetsAt: Int64?
  var id: String { key }
  enum CodingKeys: String, CodingKey {
    case key, label, percent
    case resetsAt = "resets_at"
  }
}
struct AgentUsageExtra: Decodable, Equatable {
  let label: String
  let unit: String
  let used: Double?
  let limit: Double?
  let balance: Double?
}
struct AgentUsageReport: Decodable, Equatable, Identifiable {
  let provider: String
  let profileKey: String
  let accountKey: String?
  let accountLabel: String?
  var activeSessions: Int
  let windows: [AgentUsageWindow]
  let extra: AgentUsageExtra?
  let checking: Bool
  let checkedAt: Int64?
  let lastSuccessAt: Int64?
  let error: String?
  var id: String { provider + ":" + (accountKey ?? profileKey) }
  var normalizedEmail: String? {
    let label = accountLabel?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    return label?.contains("@") == true ? label : nil
  }
  var name: String { provider == "codex" ? "Codex" : "Claude" }
  var needsKeychainAccess: Bool {
    provider == "claude" && error?.localizedCaseInsensitiveContains("Keychain") == true
  }
  var availabilityText: String {
    if checking { return "Reading usage…" }
    if needsKeychainAccess { return "Keychain access needed" }
    return error ?? "No usage limits reported"
  }
  var primaryWindows: [AgentUsageWindow] {
    let main = windows.filter { ["five_hour", "weekly"].contains($0.key) }
    return main.isEmpty ? Array(windows.prefix(2)) : main
  }
  enum CodingKeys: String, CodingKey {
    case provider, windows, extra, checking, error
    case profileKey = "profile_key"
    case accountKey = "account_key"
    case accountLabel = "account_label"
    case activeSessions = "active_sessions"
    case checkedAt = "checked_at"
    case lastSuccessAt = "last_success_at"
  }
}

struct AgentUsagePage: Identifiable, Equatable {
  let id: String
  var endpoints: [String]
  let label: String
  var reports: [AgentUsageReport]
  var sources: [String: String]

  static func build(
    reports: [String: [AgentUsageReport]], machines: [(String, String)],
    unavailable: Set<String> = []
  ) -> [Self] {
    struct Account {
      var report: AgentUsageReport
      var endpoint: String
      var endpoints: [String]
      var email: String?
    }
    var identities: [String: Set<String>] = [:]
    for (endpoint, _) in machines {
      for report in reports[endpoint] ?? [] {
        if report.accountKey != nil, let email = report.normalizedEmail {
          identities[report.provider + ":" + email, default: []].insert(report.id)
        }
      }
    }
    var accounts: [String: Account] = [:]
    for (endpoint, _) in machines {
      for report in reports[endpoint] ?? []
      where report.activeSessions > 0 || report.accountKey != nil || report.accountLabel != nil {
        let email = report.normalizedEmail
        let key: String
        if report.accountKey != nil {
          key = report.id
        } else if let email {
          let login = report.provider + ":" + email
          let matches = identities[login, default: []]
          key = matches.count == 1 ? matches.first! : "login:" + login
        } else {
          key = "profile:\(endpoint):\(report.id)"
        }
        if var account = accounts[key] {
          let sessions = account.report.activeSessions + report.activeSessions
          func rank(_ r: AgentUsageReport, _ source: String) -> (Int, Int64, Int64) {
            (
              unavailable.contains(source) || r.error != nil ? 0 : 1,
              r.lastSuccessAt ?? 0, r.checkedAt ?? 0
            )
          }
          if rank(report, endpoint) > rank(account.report, account.endpoint) {
            account.report = report
            account.endpoint = endpoint
            account.email = email ?? account.email
          }
          account.email = account.email ?? email
          account.report.activeSessions = sessions
          if !account.endpoints.contains(endpoint) { account.endpoints.append(endpoint) }
          accounts[key] = account
        } else {
          accounts[key] = Account(
            report: report, endpoint: endpoint, endpoints: [endpoint], email: email)
        }
      }
    }

    // A login can belong to multiple organizations at the same provider. Only
    // link providers by email when each has one unambiguous account identity.
    let known = accounts.values.filter { $0.email != nil }
    let ambiguous = Set(
      Dictionary(grouping: known, by: { $0.email! }).compactMap { email, group in
        Set(group.map { $0.report.provider }).count < group.count ? email : nil
      })
    var pages: [String: Self] = [:]
    for key in accounts.keys.sorted() {
      guard let account = accounts[key] else { continue }
      let report = account.report
      let linked =
        account.email != nil
        && !ambiguous.contains(account.email ?? "")
      let id = linked ? "email:" + (account.email ?? "") : key
      let machine = machines.first { $0.0 == account.endpoint }?.1 ?? account.endpoint
      var page =
        pages[id]
        ?? Self(id: id, endpoints: [], label: account.email ?? machine, reports: [], sources: [:])
      page.endpoints += account.endpoints.filter { !page.endpoints.contains($0) }
      page.sources[report.id] = account.endpoint
      page.reports.append(report)
      page.reports.sort {
        ($0.provider != "codex" ? 1 : 0, $0.id) < ($1.provider != "codex" ? 1 : 0, $1.id)
      }
      pages[id] = page
    }
    return pages.values.sorted { ($0.label, $0.id) < ($1.label, $1.id) }
  }
}
