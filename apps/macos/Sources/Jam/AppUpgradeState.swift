import Foundation

/// Native failures have an error-only envelope. A successful status response
/// also carries an error diagnostic; its phase must reach the progress reader.
enum AppUpgradeResponse {
  static func validate(_ data: Data, action: String) throws -> Data {
    if let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
      let error = object["error"] as? String,
      !(action == "status" && object["phase"] is String)
    {
      throw NSError(domain: "Jam Upgrade", code: 1, userInfo: [NSLocalizedDescriptionKey: error])
    }
    return data
  }
}

struct AppRelease: Codable, Equatable {
  struct Asset: Codable, Equatable {
    let name: String
    let platform: String
    let arch: String
    let sha256: String
    let size: UInt64
  }
  let schema: UInt32
  let version: String
  let `protocol`: String
  let minMacOS: String
  let assets: [Asset]
  enum CodingKeys: String, CodingKey {
    case schema, version, `protocol`, assets
    case minMacOS = "min_macos"
  }
}
struct AppUpgradeCheck: Codable {
  let current: String
  let available: Bool
  let release: AppRelease?
}
struct AppUpgradeStage: Codable {
  struct Staged: Codable {
    let directory: String
    let release: AppRelease
    let archive: String
  }
  struct Plan: Codable {
    struct Target: Codable, Identifiable {
      let endpoint: String
      let name: String
      let panes: [String]
      let sessions: [String: String]
      let incarnation: String
      let build: String
      let version: String?
      var id: String { endpoint }
      enum CodingKeys: String, CodingKey {
        case endpoint, name, panes, sessions, incarnation, build, version
      }
      func encode(to encoder: Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        try values.encode(endpoint, forKey: .endpoint)
        try values.encode(name, forKey: .name)
        try values.encode(panes, forKey: .panes)
        try values.encode(sessions, forKey: .sessions)
        try values.encode(incarnation, forKey: .incarnation)
        try values.encode(build, forKey: .build)
        // Rust serializes an unknown optional version as null. Preserve that
        // shape along with the consent-bound session generations.
        try values.encode(version, forKey: .version)
      }
    }
    let installation: String
    let localRunning: Bool
    let targets: [Target]
    let offline: [String]
    let appPIDs: [Int32]
    enum CodingKeys: String, CodingKey {
      case installation, targets, offline
      case localRunning = "local_running"
      case appPIDs = "app_pids"
    }
  }
  let staged: Staged
  let plan: Plan
}

/// Persist successful checks, retry deadlines and exact-version skips independently of UI views.
struct AppUpgradeSchedule: Codable {
  var nextCheck: TimeInterval = 0
  var lastSuccess: TimeInterval?
  var failures = 0
  var skippedVersion: String?
  var release: AppRelease?
  var journal: String?

  func isDue(at now: TimeInterval, automatic: Bool) -> Bool { automatic && now >= nextCheck }
  func visibleRelease(manual: Bool, current: String = "") -> AppRelease? {
    guard let release else { return nil }
    if !current.isEmpty, AppUpgradeRequirement.satisfies(current, minimum: release.version) {
      return nil
    }
    if !manual, let skippedVersion,
      AppUpgradeRequirement.satisfies(skippedVersion, minimum: release.version)
    {
      return nil
    }
    return release
  }
  mutating func succeeded(_ check: AppUpgradeCheck, at now: TimeInterval) {
    lastSuccess = now
    failures = 0
    nextCheck = now + 24 * 60 * 60
    release = check.available ? check.release : nil
  }
  mutating func failed(at now: TimeInterval) {
    failures = min(5, failures + 1)
    nextCheck = now + min(6 * 60 * 60, 15 * 60 * pow(2, Double(failures - 1)))
  }
  mutating func skip(_ version: String) { skippedVersion = version }
}

/// Candidates come from authenticated stable manifests. A stable release also
/// satisfies a prerelease minimum with the same major/minor/patch numbers.
enum AppUpgradeRequirement {
  static func satisfies(_ candidate: String, minimum: String) -> Bool {
    func core(_ version: String) -> [UInt64]? {
      let core = version.split(separator: "+", maxSplits: 1)[0]
        .split(separator: "-", maxSplits: 1)[0]
      let fields = core.split(separator: ".", omittingEmptySubsequences: false)
      guard fields.count == 3 else { return nil }
      let parts = fields.compactMap { field -> UInt64? in
        guard !field.isEmpty, field.allSatisfy({ $0.isASCII && $0.isNumber }),
          field.count == 1 || field.first != "0"
        else { return nil }
        return UInt64(field)
      }
      return parts.count == 3 ? parts : nil
    }
    guard let candidate = core(candidate), let minimum = core(minimum) else { return false }
    for (left, right) in zip(candidate, minimum) where left != right { return left > right }
    return true
  }
}

/// Background connection retries report a requirement once; a deliberate
/// reconnect is a new user request and may expose an update previously skipped.
struct AppUpgradeRequirements {
  private var versions: [String: String] = [:]
  var minimum: String? {
    versions.values.reduce(nil) { current, next in
      guard let current else { return next }
      return AppUpgradeRequirement.satisfies(next, minimum: current) ? next : current
    }
  }
  mutating func forget(endpoint: String) { versions.removeValue(forKey: endpoint) }
  mutating func retain(endpoints: Set<String>) {
    versions = versions.filter { endpoints.contains($0.key) }
  }
  mutating func observe(endpoint: String, minimum: String, manual: Bool) -> Bool {
    let changed = versions[endpoint] != minimum
    versions[endpoint] = minimum
    return manual || changed
  }
}

struct AppUpgradeRecovery: Codable, Identifiable {
  let journal: String
  let version: String
  let phase: String
  let error: String?
  var id: String { journal }
}
