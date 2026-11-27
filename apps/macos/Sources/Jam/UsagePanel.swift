import SwiftUI

extension Store {
  func authorizeUsage(profile: String, completion: @escaping (String?) -> Void) {
    DispatchQueue.global(qos: .utility).async {
      let result = rpc(["AuthorizeUsage": ["profile": profile]])
      DispatchQueue.main.async {
        self.usagePolled[""] = .distantPast
        self.refreshAgentUsage()
        completion(result["error"] as? String)
      }
    }
  }
  var usagePages: [AgentUsagePage] {
    AgentUsagePage.build(
      reports: agentUsage.mapValues {
        $0.filter { config?.enabledAgents.contains($0.provider) != false }
      }, machines: [("", "Local")] + remotes.map { ($0.id, $0.name) },
      unavailable: Set(([""] + remotes.map(\.id)).filter { usageUnavailable($0) }))
  }
  func usageUnavailable(_ endpoint: String) -> Bool {
    (endpoint.isEmpty ? !connected : remoteMessages[endpoint] != "Connected")
      || usageErrors[endpoint] != nil
  }
  func usageResetSource(for report: AgentUsageReport, preferred: String) -> UsageResetSource? {
    guard let account = report.accountKey else { return nil }
    let capability = report.provider == "claude" ? "claude_usage_resets" : "usage_reset_credits"
    let machines = [("", "Local")] + remotes.map { ($0.id, $0.name) }
    // A legacy machine can supply the freshest reading. Another logged-in
    // machine can redeem only when the provider's scoped account key matches.
    for (endpoint, name) in machines.filter({ $0.0 == preferred })
      + machines.filter({ $0.0 != preferred })
    {
      guard !usageUnavailable(endpoint),
        capabilitiesByEndpoint[endpoint]?.contains(capability) == true,
        let profile = agentUsage[endpoint]?.first(where: {
          $0.provider == report.provider && $0.accountKey == account
        })
      else { continue }
      return UsageResetSource(endpoint: endpoint, profile: profile.profileKey, machine: name)
    }
    return nil
  }
  func refreshAgentUsage() {
    for endpoint in [""] + remotes.map(\.id) {
      let available = endpoint.isEmpty ? connected : remoteMessages[endpoint] == "Connected"
      guard available, !serverBusy.contains(endpoint), !usageLoading.contains(endpoint),
        Date().timeIntervalSince(usagePolled[endpoint] ?? .distantPast) >= 3
      else { continue }
      if let capabilities = capabilitiesByEndpoint[endpoint], !capabilities.contains("agent_usage")
      {
        continue
      }
      usageLoading.insert(endpoint)
      usagePolled[endpoint] = Date()
      let known = capabilitiesByEndpoint[endpoint]
      let generation = serverGenerations[endpoint, default: 0]
      let remoteGeneration = remoteConnectionGenerations[endpoint, default: 0]
      DispatchQueue.global(qos: .utility).async {
        let capabilities =
          known ?? Set(rpc("Capabilities", endpoint: endpoint)["Capabilities"] as? [String] ?? [])
        let reply =
          capabilities.contains("agent_usage") ? rpc("AgentUsage", endpoint: endpoint) : [:]
        let reports = reply["AgentUsage"].flatMap {
          try? JSONSerialization.data(withJSONObject: $0)
        }
        .flatMap { try? JSONDecoder().decode([AgentUsageReport].self, from: $0) }
        DispatchQueue.main.async {
          self.usageLoading.remove(endpoint)
          guard self.serverGenerations[endpoint, default: 0] == generation,
            self.remoteConnectionGenerations[endpoint, default: 0] == remoteGeneration,
            endpoint.isEmpty || self.remotes.contains(where: { $0.id == endpoint })
          else { return }
          // A transient failed capabilities request must not cache an empty set.
          if !capabilities.isEmpty { self.capabilitiesByEndpoint[endpoint] = capabilities }
          if let reports {
            if self.agentUsage[endpoint] != reports { self.agentUsage[endpoint] = reports }
            self.usageErrors.removeValue(forKey: endpoint)
          } else if let error = reply["error"] as? String {
            self.usageErrors[endpoint] = error
          }
        }
      }
    }
  }
}

struct UsagePanel: View {
  @EnvironmentObject var store: Store
  @State private var selectedAccount: String?
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  private var motion: Animation? {
    reduceMotion ? nil : .timingCurve(0.22, 0.8, 0.25, 1, duration: 0.24)
  }
  private var headerHeight: CGFloat { pages.count > 1 ? 24 : 14 }
  private var pages: [AgentUsagePage] { store.usagePages }
  private var page: AgentUsagePage? {
    pages.first { $0.id == selectedAccount } ?? pages.first {
      $0.endpoints.contains(store.currentEndpoint)
    } ?? pages.first
  }

  private func open(_ report: AgentUsageReport, on page: AgentUsagePage) {
    guard let endpoint = page.sources[report.id] else { return }
    store.usageSelection = UsageSelection(
      report: report, endpoint: endpoint,
      machine: store.remotes.first { $0.id == endpoint }?.name ?? "Local")
  }

  private func move(_ direction: Int) {
    guard !pages.isEmpty else { return }
    let index = page.flatMap { current in pages.firstIndex { $0.id == current.id } } ?? 0
    selectedAccount = pages[(index + direction + pages.count) % pages.count].id
  }
  var body: some View {
    VStack(spacing: 0) {
      if let page {
        VStack(alignment: .leading, spacing: 9) {
          HStack(spacing: 4) {
            Button {
              if let report = page.reports.first { open(report, on: page) }
            } label: {
              HStack(spacing: 4) {
                Text("USAGE")
                if pages.count > 1 { Text("(\(page.label))").lineLimit(1) }
                Spacer(minLength: 0)
              }.font(.caption).foregroundStyle(.secondary).contentShape(Rectangle())
            }.accessibilityLabel("Show usage details")
            if pages.count > 1 {
              Button {
                move(-1)
              } label: {
                Image(systemName: "chevron.left").frame(width: 24, height: 24).contentShape(
                  Rectangle())
              }.help("Previous usage account")
              Button {
                move(1)
              } label: {
                Image(systemName: "chevron.right").frame(width: 24, height: 24).contentShape(
                  Rectangle())
              }.help("Next usage account")
            }
          }.buttonStyle(.plain).frame(height: headerHeight)
          VStack(alignment: .leading, spacing: 10) {
            ForEach(page.reports) { report in
              UsageCard(
                report: report, showDetails: { open(report, on: page) },
                local: page.sources[report.id] == "",
                stale: page.sources[report.id].map(store.usageUnavailable) ?? true
              ).transition(.opacity)
            }
          }.fixedSize(horizontal: false, vertical: true)
        }.padding(.horizontal, 14).padding(.vertical, 8)
          .transition(.opacity)
      }
    }.fixedSize(horizontal: false, vertical: true)
      .animation(motion, value: page)
      .popover(item: $store.usageSelection, arrowEdge: .trailing) { selection in
        UsageDetails(selection: selection, cache: store.usageResetCache).environmentObject(store)
      }
  }
}

private struct UsageCard: View {
  let report: AgentUsageReport
  let showDetails: () -> Void
  let local: Bool
  let stale: Bool
  var body: some View {
    VStack(alignment: .leading, spacing: 5) {
      Button(action: showDetails) {
        VStack(alignment: .leading, spacing: 5) {
          HStack(spacing: 5) {
            Text(report.name).font(.caption.weight(.medium))
            if let account = report.accountLabel {
              Text(account).font(.system(size: 9)).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 0)
            if report.checking {
              ProgressView().controlSize(.mini)
            } else if stale || report.error != nil {
              Image(systemName: "clock").font(.system(size: 9)).foregroundStyle(.secondary)
            }
          }
          if report.primaryWindows.isEmpty {
            Text(report.availabilityText)
              .font(.system(size: 10)).foregroundStyle(.secondary)
              .fixedSize(horizontal: false, vertical: true)
          } else {
            UsageBars(windows: report.primaryWindows)
              .opacity(stale || report.error != nil ? 0.5 : 1)
          }
        }.frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
      }.buttonStyle(.plain).help("Usage details").accessibilityLabel(
        "Show \(report.name) usage details")
      if report.needsKeychainAccess, local { UsageKeychainAccess(report: report) }
    }
  }
}

struct UsageKeychainAccess: View {
  @EnvironmentObject private var store: Store
  let report: AgentUsageReport
  @State private var authorizing = false
  @State private var authorizationError: String?

  var body: some View {
    VStack(alignment: .leading, spacing: 5) {
      Button(authorizing ? "Requesting access…" : "Allow Keychain Access") {
        authorizing = true
        authorizationError = nil
        store.authorizeUsage(profile: report.profileKey) { error in
          authorizing = false
          authorizationError = error
        }
      }.buttonStyle(.bordered).controlSize(.small).disabled(authorizing || report.checking)
      if let authorizationError {
        Text(WarningContent(authorizationError).summary).font(.system(size: 10))
          .foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
      }
    }
  }
}

struct UsageBars: View {
  let windows: [AgentUsageWindow]
  var detailed = false
  var body: some View {
    let labelFont = NSFont.monospacedSystemFont(ofSize: detailed ? 11 : 9, weight: .regular)
    let labelWidth = min(
      detailed ? 100 : 64,
      windows.reduce(CGFloat(20)) { width, window in
        max(width, ceil((window.label as NSString).size(withAttributes: [.font: labelFont]).width))
      })
    let gap: CGFloat = detailed ? 10 : 6
    return VStack(alignment: .leading, spacing: detailed ? 10 : 5) {
      ForEach(windows) { window in
        HStack(alignment: .top, spacing: gap) {
          Text(window.label).font(Font(labelFont)).foregroundStyle(.secondary).lineLimit(1)
            .frame(width: labelWidth, height: detailed ? 17 : 14, alignment: .leading).help(
              window.label)
          VStack(alignment: .leading, spacing: 2) {
            UsageMeter(percent: window.percent).frame(height: detailed ? 8 : 4)
            if detailed {
              Text(window.renewalText).font(.caption).foregroundStyle(.secondary)
            }
          }.frame(maxWidth: .infinity, alignment: .leading).padding(.top, detailed ? 4.5 : 5)
          Text(window.percent.map { String(format: "%.0f%%", $0) } ?? "—")
            .font(.system(size: detailed ? 13 : 9, weight: detailed ? .semibold : .regular))
            .monospacedDigit().foregroundStyle(detailed ? .primary : .secondary)
            .lineLimit(1).fixedSize()
            .frame(width: detailed ? 42 : 28, height: detailed ? 17 : 14, alignment: .trailing)
        }.help(window.renewalText)
      }
    }.frame(maxWidth: .infinity, alignment: .leading)
  }
}

private struct UsageMeter: View {
  @Environment(\.jamTheme) private var theme
  let percent: Double?
  var body: some View {
    GeometryReader { geometry in
      ZStack(alignment: .leading) {
        Capsule().fill(Color.primary.opacity(0.09))
        if let percent {
          Capsule().fill(Color(nsColor: theme.usageFill(percent: percent)))
            .frame(width: geometry.size.width * min(1, max(0, percent / 100)))
        }
      }
    }
  }
}
