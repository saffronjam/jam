import CJam
import SwiftUI

struct UsageSelection: Identifiable {
  let id = UUID()
  let report: AgentUsageReport
  let endpoint: String
  let machine: String
}

struct UsageResetSource: Equatable {
  let endpoint: String
  let profile: String
  let machine: String
}

struct UsageDetails: View {
  @EnvironmentObject private var store: Store
  @Environment(\.dismiss) private var dismiss
  let selection: UsageSelection
  @ObservedObject var cache: UsageResetCache
  private var cacheKey: String { selection.report.id }
  private var credits: ResetCredits? { cache.snapshot(for: cacheKey)?.credits }
  private var resetError: String? { cache.errors[cacheKey] }
  private var redeeming: Bool { cache.redeeming.contains(cacheKey) }
  @State private var confirmation: ResetCredits.Credit?
  @State private var feedback: String?
  @State private var detailsHeight: CGFloat = 0

  private var current: AgentUsageReport? {
    store.agentUsage[selection.endpoint]?.first {
      $0.profileKey == selection.report.profileKey && $0.id == selection.report.id
    }
  }
  private var report: AgentUsageReport { current ?? selection.report }
  private var stale: Bool { current == nil || store.usageUnavailable(selection.endpoint) }
  private var resetSource: UsageResetSource? {
    store.usageResetSource(for: report, preferred: selection.endpoint)
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      HStack(alignment: .firstTextBaseline, spacing: 10) {
        Text("\(report.name) usage").font(.title3.weight(.semibold)).fixedSize()
        Spacer(minLength: 0)
        if let account = report.accountLabel {
          Text(account).font(.caption).foregroundStyle(.secondary)
            .lineLimit(1).truncationMode(.middle).help(account).textSelection(.enabled)
        }
      }
      Divider()
      ScrollView {
        details.onGeometryChange(for: CGFloat.self) {
          $0.size.height
        } action: {
          detailsHeight = $0
        }
      }.scrollBounceBehavior(.basedOnSize).frame(height: min(detailsHeight, 395))
      Divider()
      HStack {
        if let updated = report.lastSuccessAt {
          Text("Updated \(timestamp(updated))").font(.caption).foregroundStyle(.secondary)
        }
        Spacer()
        Button("Done") { dismiss() }.keyboardShortcut(.defaultAction)
      }
    }
    .padding(16).frame(width: 400).frame(maxHeight: 550)
    .onChange(of: credits?.redeemable.map(\.id)) {
      if let confirmation, credits?.redeemable.contains(where: { $0.id == confirmation.id }) != true
      {
        self.confirmation = nil
      }
    }
    .task(id: resetSource) {
      guard report.accountKey != nil else { return }
      while !Task.isCancelled {
        await loadCredits()
        do { try await Task.sleep(for: .seconds(UsageResetCache.refreshInterval)) } catch { return }
      }
    }
  }

  private var details: some View {
    VStack(alignment: .leading, spacing: 14) {
      if !report.windows.isEmpty { UsageBars(windows: report.windows, detailed: true) }
      if report.windows.isEmpty && report.error == nil {
        Text(report.availabilityText).foregroundStyle(.secondary)
      }
      if let extra = report.extra {
        if let balance = extra.balance {
          Text("\(balance.formatted()) \(extra.unit)").foregroundStyle(.secondary)
        }
        if let used = extra.used {
          Text(
            "\(extra.label) · \(used.formatted())\(extra.limit.map { " / " + $0.formatted() } ?? "") \(extra.unit)"
          ).foregroundStyle(.secondary)
        }
      }
      if let error = report.error { ErrorNotice(message: error) }
      if report.needsKeychainAccess, selection.endpoint.isEmpty, current != nil {
        UsageKeychainAccess(report: report)
      }
      if stale {
        Text("Showing last known usage from \(selection.machine).").font(.caption).foregroundStyle(
          .secondary)
      }
      Divider()
      VStack(alignment: .leading, spacing: 7) {
        Text("Free resets").font(.headline)
        if let source = resetSource, source.endpoint != selection.endpoint {
          Text("Using this account on \(source.machine)").font(.caption).foregroundStyle(.secondary)
        }
        if credits == nil && resetError == nil && report.accountKey != nil && !redeeming {
          ProgressView("Checking availability…").controlSize(.small)
        }
        if let credits {
          Text(
            credits.available == 1 ? "1 reset available" : "\(credits.available) resets available")
          ForEach(credits.redeemable) { credit in
            HStack {
              VStack(alignment: .leading, spacing: 3) {
                Text(credit.title ?? "\(report.name) limit reset").fontWeight(.medium)
                if let expires = credit.expiresAt {
                  Text("Expires \(timestamp(expires))").font(.caption).foregroundStyle(.secondary)
                }
              }
              Spacer()
              if confirmation == nil {
                Button("Use reset…") { confirmation = credit }.disabled(redeeming)
              }
            }
            if let selected = confirmation, selected.id == credit.id {
              Text("Use one free reset for \(report.accountLabel ?? "this account")?")
              HStack {
                Spacer()
                Button("Cancel") { confirmation = nil }
                Button("Use free reset") {
                  confirmation = nil
                  Task { await redeem(credit) }
                }
              }
            }
          }
        }
        if redeeming { ProgressView("Redeeming reset…").controlSize(.small) }
        if let feedback { Text(feedback) }
        if let resetError {
          Text(resetError).font(.caption).foregroundStyle(.secondary)
          Button("Retry") { Task { await loadCredits(force: true) } }.disabled(redeeming)
        } else if report.accountKey == nil {
          Text("Account identity unavailable. Refresh usage after signing in.").font(.caption)
            .foregroundStyle(.secondary)
        } else if let credits, credits.available > 0 && credits.redeemable.isEmpty {
          Text("No reset is currently claimable.").font(.caption).foregroundStyle(.secondary)
        }
      }
    }.frame(maxWidth: .infinity, alignment: .leading)
  }

  private func timestamp(_ seconds: Int64) -> String {
    Date(timeIntervalSince1970: Double(seconds)).formatted(date: .abbreviated, time: .shortened)
  }

  private func request(_ method: String, additional: [String: String] = [:]) async -> [String: Any]
  {
    guard let account = selection.report.accountKey else { return [:] }
    let source = resetSource
    let provider = selection.report.provider
    let params = additional.merging(["account": account, "profile": source?.profile ?? ""]) {
      _, pinned in pinned
    }
    return await Task.detached(priority: .utility) { () -> [String: Any] in
      if let source { return rpc([method: params], endpoint: source.endpoint) }
      // Older daemons can still report an account. Use the Mac's login only
      // after the native client verifies the same provider account and organization.
      guard let data = try? JSONSerialization.data(withJSONObject: [method: params]),
        let request = String(data: data, encoding: .utf8)
      else { return [:] }
      let reply = nativeString(jam_configured_usage_reset(provider, request))
      return (try? JSONSerialization.jsonObject(with: Data(reply.utf8))) as? [String: Any] ?? [:]
    }.value
  }

  @MainActor private func loadCredits(force: Bool = false) async {
    guard selection.report.accountKey != nil else { return }
    await cache.refresh(cacheKey, force: force) {
      let reply = await request("UsageResetCredits")
      guard let reading = reply["UsageResetCredits"] else {
        throw NSError(
          domain: "JamUsage", code: 1,
          userInfo: [
            NSLocalizedDescriptionKey: reply["error"] as? String ?? "Reset availability is unknown."
          ])
      }
      let decoder = JSONDecoder()
      decoder.keyDecodingStrategy = .convertFromSnakeCase
      return try decoder.decode(
        ResetCredits.self, from: JSONSerialization.data(withJSONObject: reading))
    }
  }

  @MainActor private func redeem(_ credit: ResetCredits.Credit) async {
    guard let account = selection.report.accountKey, cache.beginRedemption(cacheKey) else { return }
    feedback = nil
    // Keep the same attempt across popover dismissal or restart if the reply is lost.
    let attemptKey = "usageReset.\(account).\(credit.id.map { "id." + $0 } ?? "next")"
    let attempt = JamDefaults.shared.string(forKey: attemptKey) ?? UUID().uuidString
    JamDefaults.shared.set(attempt, forKey: attemptKey)
    var params = ["attempt": attempt]
    params["credit"] = credit.id
    let reply = await request("RedeemUsageReset", additional: params)
    let outcome = reply["UsageResetOutcome"] as? String
    switch outcome {
    case "reset": feedback = "Free reset applied."
    case "alreadyRedeemed": feedback = "This reset was already used."
    case "nothingToReset":
      feedback = "No usage window needs a reset yet. Your reset is still available."
    case "noCredit": feedback = "This account has no free resets left."
    case "cooldown": feedback = "Claude is on cooldown. Try again later."
    case "ineligible": feedback = "This account is ineligible for this reset."
    case "unavailable": feedback = "This reset is currently unavailable."
    default:
      feedback = reply["error"] as? String ?? "\(report.name) did not confirm this reset."
    }
    if reply["UsageResetOutcome"] != nil { JamDefaults.shared.removeObject(forKey: attemptKey) }
    cache.finishRedemption(
      cacheKey, invalidate: outcome != "nothingToReset" && outcome != "cooldown")
    await loadCredits(force: true)
  }
}
