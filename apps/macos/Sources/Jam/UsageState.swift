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
  var name: String { provider == "codex" ? "Codex" : "Claude" }
  var needsKeychainAccess: Bool {
    provider == "claude" && error?.localizedCaseInsensitiveContains("Keychain") == true
  }
  var availabilityText: String {
    if checking { return "Reading usage…" }
    if needsKeychainAccess { return "Keychain access needed" }
    return error ?? "No usage limits reported"
  }
  var sidebarHeight: CGFloat {
    let body = primaryWindows.isEmpty ? (checking ? 40 : 60) : 32 + primaryWindows.count * 18
    return CGFloat(body + (needsKeychainAccess ? 28 : 0))
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
  var names: [String]
  var reports: [AgentUsageReport]
  var label: String { names.joined(separator: " · ") }

  static func build(reports: [String: [AgentUsageReport]], machines: [(String, String)]) -> [Self] {
    var pages: [Self] = []
    for (endpoint, name) in machines {
      var cards: [AgentUsageReport] = []
      for report in reports[endpoint] ?? [] where report.activeSessions > 0 {
        if let index = cards.firstIndex(where: { $0.id == report.id }) {
          let sessions = cards[index].activeSessions + report.activeSessions
          if (report.lastSuccessAt ?? 0) > (cards[index].lastSuccessAt ?? 0) {
            cards[index] = report
          }
          cards[index].activeSessions = sessions
        } else {
          cards.append(report)
        }
      }
      guard !cards.isEmpty else { continue }
      cards.sort {
        ($0.provider == "codex" ? 0 : 1, $0.id) < ($1.provider == "codex" ? 0 : 1, $1.id)
      }
      // Unknown identities stay machine-local. Percentages and token values are
      // never used to guess that two machines share an account.
      let known = cards.allSatisfy { $0.accountKey != nil && $0.error == nil }
      let id = known ? cards.map(\.id).joined(separator: "|") : "machine:" + endpoint
      if known, let index = pages.firstIndex(where: { $0.id == id }) {
        pages[index].endpoints.append(endpoint)
        pages[index].names.append(name)
        for card in cards {
          guard let position = pages[index].reports.firstIndex(where: { $0.id == card.id }) else {
            continue
          }
          let sessions = pages[index].reports[position].activeSessions + card.activeSessions
          if (card.lastSuccessAt ?? 0) > (pages[index].reports[position].lastSuccessAt ?? 0) {
            pages[index].reports[position] = card
          }
          pages[index].reports[position].activeSessions = sessions
        }
      } else {
        pages.append(Self(id: id, endpoints: [endpoint], names: [name], reports: cards))
      }
    }
    return pages
  }
}
