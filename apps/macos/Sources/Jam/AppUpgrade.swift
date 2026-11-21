import AppKit
import CJam
import SwiftUI

private func upgradeReply(_ request: [String: Any]) throws -> Data {
  let payload = try JSONSerialization.data(withJSONObject: request)
  let response = Data(nativeString(jam_upgrade(String(decoding: payload, as: UTF8.self))).utf8)
  return try AppUpgradeResponse.validate(response, action: request["action"] as? String ?? "")
}
private func upgradeObject<T: Encodable>(_ value: T) throws -> Any {
  try JSONSerialization.jsonObject(with: JSONEncoder().encode(value))
}

final class AppUpgradeController: ObservableObject {
  enum Phase { case idle, checking, staging, ready, launching, applying }
  @Published var phase: Phase = .idle
  @Published var available: AppRelease?
  @Published var showSheet = false
  @Published var staged: AppUpgradeStage?
  @Published var error = ""
  @Published var notice = ""
  @Published private(set) var releaseNotesVersion: String?
  var releaseNotesURL: URL? { releaseNotesVersion.flatMap { AppReleaseNotes.url(version: $0) } }
  func dismissNotice() {
    notice = ""
    releaseNotesVersion = nil
  }
  func openReleaseNotes() {
    guard let url = releaseNotesURL else { return }
    NSWorkspace.shared.open(url)
  }
  @Published var managed = false
  @Published var requiredMinimum: String?
  @Published var pendingRecoveries: [AppUpgradeRecovery] = []
  @Published var recovery: AppUpgradeRecovery?
  @Published var showRecoveryNotice = true
  private var scanningRecoveries = false
  private var recoveriesKnown = false
  private var lastLaunch = Date.distantPast
  private let engine: ([String: Any]) throws -> Data
  init() { engine = upgradeReply }
  init(engine: @escaping ([String: Any]) throws -> Data) { self.engine = engine }
  private var requirements = AppUpgradeRequirements()
  private var requirementError = false
  private var eligibilityKnown = false
  private var pendingManualRequirement = false
  private var automatic = true
  private var configured = false
  private var generation: UInt64 = 0
  private var polling = false
  private var timer: Timer?
  private var schedule: AppUpgradeSchedule = {
    guard let data = JamDefaults.shared.data(forKey: "appUpgradeState"),
      let value = try? JSONDecoder().decode(AppUpgradeSchedule.self, from: data)
    else {
      return AppUpgradeSchedule()
    }
    return value
  }()
  deinit { timer?.invalidate() }
  var busy: Bool {
    !pendingRecoveries.isEmpty || schedule.journal != nil
      || [.checking, .staging, .launching, .applying].contains(phase)
  }
  var canLaunch: Bool {
    phase == .ready && staged != nil && pendingRecoveries.isEmpty
      && recoveriesKnown && !scanningRecoveries && schedule.journal == nil
  }
  var currentVersion: String {
    Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? ""
  }
  var settingsRelease: AppRelease? {
    guard let release = available ?? schedule.visibleRelease(manual: true, current: currentVersion)
    else { return nil }
    guard let minimum = requiredMinimum else { return release }
    return AppUpgradeRequirement.satisfies(release.version, minimum: minimum) ? release : nil
  }
  var canPrepareUpdate: Bool {
    managed && !busy && phase == .idle && settingsRelease != nil
  }
  func prepareFromSettings() {
    guard canPrepareUpdate else { return }
    available = settingsRelease
    prepare()
  }

  func configure(automatic: Bool) {
    let newlyEnabled = !self.automatic && automatic
    self.automatic = automatic
    guard !configured else {
      if newlyEnabled, eligibilityKnown, requiredMinimum != nil {
        check(manual: false, requirement: true)
      }
      return
    }
    configured = true
    DispatchQueue.global(qos: .utility).async {
      let valid = (try? self.engine(["action": "eligibility"])) != nil
      DispatchQueue.main.async {
        self.managed = valid
        self.eligibilityKnown = true
        self.scanRecoveries()
        if self.requiredMinimum != nil {
          self.check(manual: self.pendingManualRequirement, requirement: true)
        }
        guard valid else { return }
        self.timer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
          self?.tick()
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 8) { [weak self] in
          guard let self else { return }
          if self.automatic && self.requiredMinimum == nil {
            self.available = self.schedule.visibleRelease(
              manual: false, current: self.currentVersion)
          }
          self.tick()
        }
      }
    }
  }
  private func persist() {
    if let data = try? JSONEncoder().encode(schedule) {
      JamDefaults.shared.set(data, forKey: "appUpgradeState")
    }
  }
  private func tick() {
    scanRecoveries()
    if !pendingRecoveries.isEmpty { return }
    if schedule.journal != nil {
      poll()
      return
    }
    if schedule.isDue(at: Date().timeIntervalSince1970, automatic: automatic) {
      check(manual: false)
    }
  }
  func forgetRequirement(endpoint: String) {
    requirements.forget(endpoint: endpoint)
    reconcileRequirements()
  }
  func retainRequirements(endpoints: Set<String>) {
    requirements.retain(endpoints: endpoints)
    reconcileRequirements()
  }
  private func reconcileRequirements() {
    guard requiredMinimum != requirements.minimum else { return }
    requiredMinimum = requirements.minimum
    if requiredMinimum == nil { pendingManualRequirement = false }
    if requirementError {
      error = ""
      requirementError = false
    }
    guard managed else { return }
    let candidate = schedule.visibleRelease(manual: false, current: currentVersion)
    available = candidate.flatMap { release in
      guard let minimum = requiredMinimum else { return release }
      return AppUpgradeRequirement.satisfies(release.version, minimum: minimum) ? release : nil
    }
  }
  func requireClientUpgrade(endpoint: String, minimum: String, manual: Bool) {
    if manual { pendingManualRequirement = true }
    let shouldCheck = requirements.observe(endpoint: endpoint, minimum: minimum, manual: manual)
    requiredMinimum = requirements.minimum
    guard shouldCheck else { return }
    guard manual || automatic else { return }
    if phase == .staging || phase == .ready { cancel() }
    guard eligibilityKnown else {
      configure(automatic: automatic)
      return
    }
    check(manual: manual, requirement: true)
  }
  func check(manual: Bool = true, requirement: Bool = false) {
    guard phase == .idle, schedule.journal == nil else { return }
    if manual {
      error = ""
      requirementError = false
      dismissNotice()
    }
    generation &+= 1
    let token = generation
    phase = .checking
    DispatchQueue.global(qos: .utility).async {
      let reply = Result {
        try JSONDecoder().decode(AppUpgradeCheck.self, from: self.engine(["action": "check"]))
      }
      DispatchQueue.main.async {
        guard self.generation == token else { return }
        self.phase = .idle
        let manualPresentation = manual || self.pendingManualRequirement
        self.pendingManualRequirement = false
        switch reply {
        case .success(let value):
          if self.requirementError {
            self.error = ""
            self.requirementError = false
          }
          self.schedule.succeeded(value, at: Date().timeIntervalSince1970)
          self.available = self.schedule.visibleRelease(
            manual: manualPresentation, current: self.currentVersion)
          if !self.managed {
            self.available = nil
          }
          if let minimum = self.requiredMinimum {
            let skippedSatisfiesRequirement =
              self.schedule.skippedVersion.map {
                AppUpgradeRequirement.satisfies($0, minimum: minimum)
              } ?? false
            let notify = manualPresentation || (requirement && !skippedSatisfiesRequirement)
            if let release = value.release, value.available,
              AppUpgradeRequirement.satisfies(release.version, minimum: minimum)
            {
              if !self.managed && notify {
                self.requirementError = true
                self.error =
                  "This machine requires Jam \(minimum). Install the release Jam.app before connecting; development builds cannot self-upgrade."
              }
            } else {
              self.available = nil
              if notify {
                self.requirementError = true
                self.error =
                  "This machine requires Jam \(minimum), but no matching published update is available. The remote server was left unchanged."
              }
            }
          } else if manualPresentation && !value.available {
            self.notice = "Jam \(value.current) is up to date."
          }
        case .failure(let failure):
          self.schedule.failed(at: Date().timeIntervalSince1970)
          let skippedRequirement =
            self.requiredMinimum.map { minimum in
              self.schedule.skippedVersion.map {
                AppUpgradeRequirement.satisfies($0, minimum: minimum)
              } ?? false
            } ?? false
          if manualPresentation || (requirement && !skippedRequirement) {
            let context = self.requiredMinimum.map { "This machine requires Jam \($0). " } ?? ""
            self.requirementError = self.requiredMinimum != nil
            self.error =
              context + "Could not check for Jam updates.\n" + failure.localizedDescription
          }
        }
        self.persist()
      }
    }
  }
  func skip() {
    guard let available else { return }
    schedule.skip(available.version)
    persist()
    self.available = nil
  }
  func prepare() {
    if let interrupted = pendingRecoveries.first {
      recovery = interrupted
      return
    }
    guard !busy, phase == .idle, let release = available else { return }
    error = ""
    requirementError = false
    dismissNotice()
    staged = nil
    showSheet = true
    guard managed else {
      error = "Install the release Jam.app before upgrading. Development builds cannot be replaced."
      return
    }
    generation &+= 1
    let token = generation
    phase = .staging
    DispatchQueue.global(qos: .userInitiated).async {
      let reply = Result {
        try JSONDecoder().decode(
          AppUpgradeStage.self,
          from: self.engine([
            "action": "stage", "release": try upgradeObject(release),
          ]))
      }
      DispatchQueue.main.async {
        guard self.generation == token else { return }
        switch reply {
        case .success(let value):
          self.staged = value
          self.phase = .ready
        case .failure(let failure):
          self.phase = .idle
          self.error = "Could not prepare the Jam upgrade.\n" + failure.localizedDescription
        }
      }
    }
  }
  func cancel() {
    guard phase != .launching && phase != .applying else { return }
    generation &+= 1
    phase = .idle
    staged = nil
    showSheet = false
  }
  func launch() {
    guard canLaunch, let staged else { return }
    phase = .launching
    lastLaunch = Date()
    DispatchQueue.global(qos: .userInitiated).async {
      let reply = Result {
        try JSONSerialization.jsonObject(
          with: self.engine([
            "action": "launch", "staged": try upgradeObject(staged.staged),
            "plan": try upgradeObject(staged.plan), "confirmed": true,
          ])) as? [String: Any]
      }
      DispatchQueue.main.async {
        switch reply {
        case .success(let value):
          guard let path = value?["journal"] as? String else {
            self.phase = .ready
            self.error = "The updater did not return its progress journal."
            return
          }
          self.schedule.journal = path
          self.lastLaunch = Date()
          self.persist()
          self.phase = .applying
          self.poll()
        case .failure(let failure):
          self.phase = .ready
          self.error = "Could not start the Jam upgrade.\n" + failure.localizedDescription
        }
      }
    }
  }
  private func scanRecoveries() {
    guard !scanningRecoveries, phase != .launching,
      Date().timeIntervalSince(lastLaunch) > 5
    else { return }
    let launch = lastLaunch
    scanningRecoveries = true
    DispatchQueue.global(qos: .utility).async {
      let result = Result {
        try JSONDecoder().decode(
          [AppUpgradeRecovery].self,
          from: self.engine(["action": "pending"]))
      }
      DispatchQueue.main.async {
        self.scanningRecoveries = false
        guard self.lastLaunch == launch, self.phase != .launching else { return }
        switch result {
        case .success(let values):
          self.recoveriesKnown = true
          let previous = Set(self.pendingRecoveries.map(\.id))
          self.pendingRecoveries = values
          if values.contains(where: { !previous.contains($0.id) }) {
            self.showRecoveryNotice = true
          }
          if !values.isEmpty && (self.phase == .staging || self.phase == .ready) {
            self.cancel()
          }
          if values.contains(where: { $0.journal == self.schedule.journal }) {
            self.phase = .idle
            self.showSheet = false
          }
        case .failure(let failure):
          self.recoveriesKnown = false
          self.error =
            "Could not inspect interrupted Jam upgrades.\n" + failure.localizedDescription
        }
      }
    }
  }
  func recover() {
    guard let recovery, phase == .idle || phase == .ready || phase == .checking else { return }
    // A read-only network check must not delay explicit local recovery or
    // overwrite its progress when that old check eventually finishes.
    generation &+= 1
    phase = .launching
    lastLaunch = Date()
    error = ""
    dismissNotice()
    requirementError = false
    DispatchQueue.global(qos: .userInitiated).async {
      let reply = Result {
        try JSONSerialization.jsonObject(
          with: self.engine([
            "action": "recover", "journal": recovery.journal, "confirmed": true,
          ])) as? [String: Any]
      }
      DispatchQueue.main.async {
        switch reply {
        case .success(let value):
          guard let path = value?["journal"] as? String else {
            self.phase = .idle
            self.error = "Recovery did not return its progress journal."
            return
          }
          self.pendingRecoveries.removeAll { $0.journal == path }
          self.schedule.journal = path
          self.lastLaunch = Date()
          self.persist()
          self.phase = .applying
          self.poll()
        case .failure(let failure):
          self.phase = .idle
          self.error = "Could not start upgrade recovery.\n" + failure.localizedDescription
        }
      }
    }
  }
  private func poll() {
    guard !polling, let path = schedule.journal,
      !pendingRecoveries.contains(where: { $0.journal == path })
    else { return }
    polling = true
    DispatchQueue.global(qos: .utility).async {
      let reply = Result {
        try JSONSerialization.jsonObject(with: self.engine(["action": "status", "journal": path]))
          as? [String: Any]
      }
      DispatchQueue.main.async {
        self.polling = false
        switch reply {
        case .success(let value):
          let phase = value?["phase"] as? String ?? ""
          let details = value?["error"] as? String
          if [
            "complete", "complete_with_pending", "failed", "rolled_back", "recovered", "abandoned",
          ].contains(phase) {
            self.schedule.journal = nil
            self.pendingRecoveries.removeAll { $0.journal == path }
            self.recovery = nil
            self.phase = .idle
            self.showSheet = false
            self.releaseNotesVersion = nil
            if phase == "recovered" || phase == "abandoned" {
              self.notice =
                phase == "recovered"
                ? "The interrupted upgrade was recovered." : "The upgrade was cancelled."
              if let details, !details.isEmpty { self.notice += " " + details }
            } else if phase == "complete" {
              self.available = nil
              self.schedule.release = nil
              self.releaseNotesVersion = value?["version"] as? String ?? self.currentVersion
              self.notice =
                "Jam was upgraded to \(value?["version"] as? String ?? self.currentVersion)."
            } else {
              if phase == "complete_with_pending" {
                self.available = nil
                self.schedule.release = nil
              }
              self.error =
                (phase == "complete_with_pending"
                  ? "Jam was upgraded; some machines still need attention."
                  : "The Jam upgrade did not complete.") + (details.map { "\n" + $0 } ?? "")
            }
            self.persist()
            if phase == "failed" || phase == "rolled_back" {
              DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
                self?.scanRecoveries()
              }
            }
          } else {
            self.phase = .applying
            if Date().timeIntervalSince(self.lastLaunch) > 5 { self.scanRecoveries() }
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in self?.poll() }
          }
        case .failure(let failure):
          self.phase = .applying
          self.error = "Could not read upgrade progress.\n" + failure.localizedDescription
        // Keep the journal for the next startup or retry; never launch a second updater.
        }
      }
    }
  }
}

struct AppUpgradeOverlay: View {
  @ObservedObject var controller: AppUpgradeController
  var showsNotification = true
  var body: some View {
    VStack(alignment: .trailing, spacing: 8) {
      if controller.showRecoveryNotice && !controller.showSheet,
        controller.recovery == nil, let interrupted = controller.pendingRecoveries.first
      {
        VStack(alignment: .leading, spacing: 12) {
          Text("A Jam upgrade was interrupted").font(.callout.weight(.semibold))
          Text("Recover the installation before starting another upgrade.").font(.callout)
            .foregroundStyle(.secondary)
          HStack {
            Spacer()
            Button("Later") { controller.showRecoveryNotice = false }
            Button("Recover…") { controller.recovery = interrupted }
          }.controlSize(.small)
        }.padding(14).frame(width: 330)
          .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
      } else if showsNotification && !controller.showSheet && !controller.error.isEmpty {
        WarningToast(message: controller.error, retry: nil, dismiss: { controller.error = "" })
      } else if controller.managed && showsNotification && !controller.showSheet
        && !controller.notice.isEmpty
      {
        VStack(alignment: .leading, spacing: 12) {
          HStack(alignment: .top) {
            Text(controller.notice).font(.callout)
            Spacer(minLength: 8)
            Button(action: controller.dismissNotice) {
              Image(systemName: "xmark")
            }
            .buttonStyle(.plain).accessibilityLabel("Dismiss update notification")
          }
          if controller.releaseNotesURL != nil {
            HStack {
              Spacer()
              Button("Open Release Notes", action: controller.openReleaseNotes)
            }.controlSize(.small)
          }
        }.padding(14).background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
      } else if let release = controller.available, controller.managed, showsNotification,
        !controller.showSheet
      {
        VStack(alignment: .leading, spacing: 12) {
          Text("Jam \(release.version) is available").font(.callout.weight(.semibold))
          if let minimum = controller.requiredMinimum {
            Text("The remote machine needs Jam \(minimum) or newer before connecting.")
              .font(.callout).foregroundStyle(.secondary)
          }
          HStack {
            Spacer()
            Button("Skip This Version", action: controller.skip).disabled(controller.busy)
            Button("Upgrade…", action: controller.prepare).disabled(controller.busy)
          }.controlSize(.small)
        }.padding(14).frame(width: 330)
          .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
      }
    }.frame(maxWidth: 360).padding(12)
      .sheet(isPresented: $controller.showSheet) { AppUpgradeSheet(controller: controller) }
      .sheet(item: $controller.recovery) { interrupted in
        AppUpgradeRecoverySheet(controller: controller, interrupted: interrupted)
      }
  }
}
struct AppUpgradeSheet: View {
  @ObservedObject var controller: AppUpgradeController
  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      Text("Upgrade Jam").font(.headline)
      if controller.phase == .staging {
        ProgressView("Downloading and verifying the update…")
      } else if controller.phase == .launching || controller.phase == .applying {
        ProgressView("Installing the update and restarting Jam…")
      } else if let stage = controller.staged {
        Text("Jam \(stage.staged.release.version)").font(.title3.weight(.semibold))
        Text(
          "Jam and the following servers will restart. Running shell jobs may end. Workspace layouts and saved agent conversations are preserved."
        )
        .foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
        ScrollView {
          VStack(alignment: .leading, spacing: 10) {
            HStack {
              Text("Jam app instances")
              Spacer()
              Text("\(stage.plan.appPIDs.count)").foregroundStyle(.secondary)
            }
            ForEach(stage.plan.targets) { target in
              HStack {
                Text(target.name)
                Spacer()
                Text("\(target.panes.count) running terminal\(target.panes.count == 1 ? "" : "s")")
                  .foregroundStyle(.secondary)
              }
            }
            if !stage.plan.offline.isEmpty {
              Divider()
              Text(
                "Offline machines will need confirmation when reachable: "
                  + stage.plan.offline.joined(separator: ", ")
              )
              .font(.callout).foregroundStyle(.secondary)
            }
          }
        }.frame(maxHeight: 180)
      }
      if !controller.error.isEmpty { ErrorNotice(message: controller.error) }
      Divider()
      HStack {
        Spacer()
        Button("Cancel", action: controller.cancel)
          .keyboardShortcut(.cancelAction).disabled(
            controller.phase == .launching || controller.phase == .applying)
        Button("Upgrade and Restart", action: controller.launch)
          .keyboardShortcut(.defaultAction).disabled(!controller.canLaunch)
      }
    }.padding(24).frame(width: 540)
      .interactiveDismissDisabled(controller.phase == .launching || controller.phase == .applying)
      .onDisappear {
        if controller.phase == .staging || controller.phase == .ready { controller.cancel() }
      }
  }
}
struct AppUpgradeRecoverySheet: View {
  @ObservedObject var controller: AppUpgradeController
  let interrupted: AppUpgradeRecovery
  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      Text("Recover Interrupted Upgrade").font(.headline)
      Text(
        "The upgrade to Jam \(interrupted.version) did not finish. Recovery will verify and repair the local installation, then relaunch Jam if needed. A responding server keeps running. Workspace layouts and saved conversations are preserved."
      )
      .fixedSize(horizontal: false, vertical: true)
      if let error = interrupted.error, !error.isEmpty { ErrorNotice(message: error) }
      if !controller.error.isEmpty { ErrorNotice(message: controller.error) }
      if controller.phase == .launching || controller.phase == .applying {
        ProgressView("Recovering the installation…")
      }
      Divider()
      HStack {
        Spacer()
        Button("Cancel") { controller.recovery = nil }
          .keyboardShortcut(.cancelAction)
          .disabled(controller.phase == .launching || controller.phase == .applying)
        Button("Recover", action: controller.recover)
          .keyboardShortcut(.defaultAction)
          .disabled(
            controller.phase != .idle && controller.phase != .ready && controller.phase != .checking
          )
      }
    }.padding(24).frame(width: 540)
      .interactiveDismissDisabled(controller.phase == .launching || controller.phase == .applying)
  }
}
struct AppUpdateSettings: View {
  @ObservedObject var controller: AppUpgradeController
  @Binding var automatic: Bool
  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      Toggle("Automatically check for updates", isOn: $automatic)
      HStack {
        Text("Jam \(controller.currentVersion)").foregroundStyle(.secondary)
        Spacer()
        if controller.phase == .checking { ProgressView().controlSize(.small) }
        Button("Check for Updates…") { controller.check() }.disabled(controller.busy)
      }
      if let interrupted = controller.pendingRecoveries.first {
        Button("Recover Interrupted Upgrade…") { controller.recovery = interrupted }
          .disabled(controller.phase == .launching || controller.phase == .applying)
      }
      if let release = controller.settingsRelease {
        HStack {
          Text("Jam \(release.version) is available").foregroundStyle(.secondary)
          Spacer()
          Button("Upgrade…", action: controller.prepareFromSettings).disabled(
            !controller.canPrepareUpdate)
        }
      } else if !controller.notice.isEmpty {
        Text(controller.notice).foregroundStyle(.secondary)
      }
      if !controller.error.isEmpty { ErrorNotice(message: controller.error) }
    }
    .sheet(isPresented: $controller.showSheet) { AppUpgradeSheet(controller: controller) }
    .sheet(item: $controller.recovery) { interrupted in
      AppUpgradeRecoverySheet(controller: controller, interrupted: interrupted)
    }
  }
}

/// Keep recovery visible when the interrupted server also reports a connection
/// error, and keep both sheets attached while ordinary notifications change.
struct AppStatusOverlay: View {
  @ObservedObject var store: Store
  @ObservedObject var controller: AppUpgradeController
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  var body: some View {
    let recoveryNotice =
      controller.showRecoveryNotice && !controller.pendingRecoveries.isEmpty
      && controller.recovery == nil && !controller.showSheet
    VStack(alignment: .trailing, spacing: 8) {
      if !store.error.isEmpty && !recoveryNotice {
        WarningToast(
          message: store.error, retry: store.connected ? nil : { store.start() },
          dismiss: { store.dismissWarning() }
        )
        .padding(12)
        .transition(reduceMotion ? .opacity : .move(edge: .bottom).combined(with: .opacity))
      }
      // Settings owns its nested upgrade sheets while it is open.
      if !store.showSettings {
        // Connection errors must not hide the action that can resolve them.
        AppUpgradeOverlay(controller: controller)
      }
    }.animation(reduceMotion ? nil : .easeOut(duration: 0.2), value: store.error.isEmpty)
  }
}
