import SwiftUI

struct ResetCredits: Codable {
  let available: Int
  let credits: [Credit]?

  var redeemable: [Credit] {
    guard available > 0 else { return [] }
    return credits ?? [Credit(creditId: nil, title: nil, expiresAt: nil)]
  }

  struct Credit: Identifiable, Codable {
    let creditId: String?
    let title: String?
    let expiresAt: Int64?
    var id: String? { creditId }
  }
}

final class UsageResetCache: ObservableObject {
  struct Snapshot: Codable {
    let credits: ResetCredits
    let checked: Date
  }

  static let refreshInterval: TimeInterval = 300
  private let defaults: UserDefaults
  @Published private var values: [String: Snapshot] = [:]
  @Published private(set) var errors: [String: String] = [:]
  @Published private(set) var redeeming = Set<String>()
  private var reads: [String: UUID] = [:]
  private var retryAfter: [String: Date] = [:]

  init(defaults: UserDefaults = JamDefaults.shared) { self.defaults = defaults }

  func snapshot(for id: String) -> Snapshot? {
    if let value = values[id] { return value }
    guard let data = defaults.data(forKey: "usageResetCache." + id) else { return nil }
    return try? JSONDecoder().decode(Snapshot.self, from: data)
  }

  @MainActor func refresh(
    _ id: String, force: Bool = false, now: Date = .now,
    fetch: @escaping @MainActor () async throws -> ResetCredits
  ) async {
    guard reads[id] == nil, !redeeming.contains(id) else { return }
    if !force {
      if let after = retryAfter[id], now < after { return }
      if let snapshot = snapshot(for: id),
        now.timeIntervalSince(snapshot.checked) >= 0,
        now.timeIntervalSince(snapshot.checked) < Self.refreshInterval
      {
        return
      }
    }
    let read = UUID()
    reads[id] = read
    errors.removeValue(forKey: id)
    // The account owns the request, so closing the popover can still warm its cache.
    await Task { @MainActor in
      defer { if reads[id] == read { reads.removeValue(forKey: id) } }
      do {
        let result = try await fetch()
        guard reads[id] == read else { return }
        let snapshot = Snapshot(credits: result, checked: now)
        values[id] = snapshot
        if let data = try? JSONEncoder().encode(snapshot) {
          defaults.set(data, forKey: "usageResetCache." + id)
        }
        retryAfter.removeValue(forKey: id)
      } catch {
        guard reads[id] == read else { return }
        errors[id] = error.localizedDescription
        retryAfter[id] = now.addingTimeInterval(30)
      }
    }.value
  }

  @MainActor func beginRedemption(_ id: String) -> Bool {
    guard redeeming.insert(id).inserted else { return false }
    reads.removeValue(forKey: id)
    errors.removeValue(forKey: id)
    return true
  }

  @MainActor func finishRedemption(_ id: String, invalidate: Bool) {
    if invalidate {
      values.removeValue(forKey: id)
      defaults.removeObject(forKey: "usageResetCache." + id)
    }
    retryAfter.removeValue(forKey: id)
    redeeming.remove(id)
  }
}
