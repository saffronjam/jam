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
      }, machines: [("", "Local")] + remotes.map { ($0.id, $0.name) })
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
  @State private var selectedEndpoint: String?
  @State private var contentHeights: [String: CGFloat] = [:]
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  private var motion: Animation? {
    reduceMotion ? nil : .timingCurve(0.22, 0.8, 0.25, 1, duration: 0.24)
  }
  private var contentHeight: CGFloat {
    guard let page else { return 0 }
    return min(
      240, contentHeights[page.id] ?? page.reports.reduce(CGFloat.zero) { $0 + $1.sidebarHeight })
  }
  private var headerHeight: CGFloat { pages.count > 1 ? 24 : 14 }
  private var panelHeight: CGFloat { page == nil ? 0 : headerHeight + 9 + contentHeight + 16 }
  private var pages: [AgentUsagePage] { store.usagePages }
  private var page: AgentUsagePage? {
    pages.first { $0.endpoints.contains(selectedEndpoint ?? store.currentEndpoint) } ?? pages.first
  }

  private func move(_ direction: Int) {
    guard !pages.isEmpty else { return }
    let index = page.flatMap { current in pages.firstIndex { $0.id == current.id } } ?? 0
    selectedEndpoint = pages[(index + direction + pages.count) % pages.count].endpoints.first
  }
  var body: some View {
    VStack(spacing: 0) {
      if let page {
        VStack(alignment: .leading, spacing: 9) {
          HStack(spacing: 4) {
            Text("USAGE").font(.caption).foregroundStyle(.secondary)
            if pages.count > 1 {
              Text("(\(page.label))").font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 0)
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
          ScrollView {
            VStack(alignment: .leading, spacing: 10) {
              ForEach(page.reports) { report in
                UsageCard(
                  report: report,
                  authorize: page.endpoints == [""]
                    ? { completion in
                      store.authorizeUsage(profile: report.profileKey, completion: completion)
                    } : nil,
                  stale: page.endpoints.allSatisfy {
                    ($0.isEmpty ? !store.connected : store.remoteMessages[$0] != "Connected")
                      || store.usageErrors[$0] != nil
                  }
                ).transition(.opacity)
              }
            }.fixedSize(horizontal: false, vertical: true)
              .background(
                GeometryReader { geometry in
                  Color.clear.preference(key: UsageContentHeight.self, value: geometry.size.height)
                })
          }.scrollIndicators(.hidden)
            .frame(height: contentHeight)
            .onPreferenceChange(UsageContentHeight.self) { height in
              if height > 0 && abs((contentHeights[page.id] ?? 0) - height) > 0.5 {
                contentHeights[page.id] = height
              }
            }
        }.padding(.horizontal, 14).padding(.vertical, 8)
          .transition(.opacity)
      }
    }.frame(height: panelHeight, alignment: .top).clipped()
      .animation(motion, value: page)
      .animation(motion, value: panelHeight)
  }
}

private struct UsageContentHeight: PreferenceKey {
  static var defaultValue: CGFloat = 0
  static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
    value = max(value, nextValue())
  }
}

private struct UsageCard: View {
  let report: AgentUsageReport
  let authorize: ((@escaping (String?) -> Void) -> Void)?
  let stale: Bool
  @State private var details = false
  @State private var authorizing = false
  @State private var authorizationError: String?
  private func requestAuthorization() {
    guard let authorize else { return }
    authorizing = true
    authorizationError = nil
    authorize { error in
      authorizing = false
      authorizationError = error
    }
  }
  var body: some View {
    VStack(alignment: .leading, spacing: 5) {
      Button {
        details.toggle()
      } label: {
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
        }.contentShape(Rectangle())
      }.buttonStyle(.plain).help("Usage details")
        .popover(isPresented: $details) {
          VStack(alignment: .leading, spacing: 10) {
            Text("\(report.name) usage").font(.headline)
            if let account = report.accountLabel {
              Text(account).font(.caption).textSelection(.enabled)
            }
            if report.accountKey == nil {
              Text("Account identity unavailable. Shown separately for this machine.").font(
                .caption
              ).foregroundStyle(.secondary)
            }
            ForEach(report.windows) { window in UsageBar(window: window) }
            if let extra = report.extra {
              if let balance = extra.balance {
                Text("\(extra.label): \(balance.formatted()) \(extra.unit)")
              }
              if let used = extra.used {
                Text(
                  "\(extra.label): \(used.formatted())\(extra.limit.map { " / " + $0.formatted() } ?? "") \(extra.unit)"
                )
              }
            }
            if let error = report.error {
              ErrorNotice(message: error)
              if report.needsKeychainAccess, authorize != nil {
                Button("Allow Keychain Access", action: requestAuthorization)
                  .disabled(authorizing || report.checking)
                if let authorizationError { ErrorNotice(message: authorizationError) }
              }
            } else if stale {
              Text("Machine unavailable. Showing last known usage.").font(.caption).foregroundStyle(
                .secondary)
            }
            if let success = report.lastSuccessAt {
              Text(
                "Updated \(Date(timeIntervalSince1970: Double(success)).formatted(date: .omitted, time: .shortened)) · \(report.activeSessions) active sessions"
              )
              .font(.caption).foregroundStyle(.secondary)
            }
          }.padding(16).frame(width: 310)
        }
      if report.primaryWindows.isEmpty {
        Text(report.availabilityText)
          .font(.system(size: 10)).foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      } else {
        ForEach(report.primaryWindows) { window in UsageBar(window: window) }
          .opacity(stale || report.error != nil ? 0.5 : 1)
      }
      if report.needsKeychainAccess, authorize != nil {
        Button(
          authorizing ? "Requesting access…" : "Allow Keychain Access", action: requestAuthorization
        )
        .buttonStyle(.bordered).controlSize(.small)
        .disabled(authorizing || report.checking)
      }
      if let authorizationError {
        Text(WarningContent(authorizationError).summary).font(.system(size: 10))
          .foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
      }
    }
  }
}

private struct UsageBar: View {
  let window: AgentUsageWindow
  var body: some View {
    HStack(spacing: 6) {
      Text(window.label).font(.system(size: 9, design: .monospaced)).foregroundStyle(.secondary)
        .lineLimit(1).frame(minWidth: 20, alignment: .leading)
      GeometryReader { geometry in
        ZStack(alignment: .leading) {
          Capsule().fill(Color.primary.opacity(0.08))
          if let percent = window.percent {
            Capsule().fill(percent >= 90 ? Color.orange : Color.accentColor.opacity(0.65))
              .frame(width: geometry.size.width * min(1, max(0, percent / 100)))
          }
        }
      }.frame(height: 4)
      Text(window.percent.map { String(format: "%.0f%%", $0) } ?? "—")
        .font(.system(size: 9, design: .monospaced)).foregroundStyle(.secondary)
        .frame(minWidth: 28, alignment: .trailing)
    }.frame(height: 14)
      .help(
        window.resetsAt.map {
          "Resets "
            + Date(timeIntervalSince1970: Double($0)).formatted(
              date: .abbreviated, time: .shortened)
        } ?? "Reset time unavailable")
  }
}
