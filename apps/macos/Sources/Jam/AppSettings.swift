import AppKit
import CJam
import SwiftUI

private func configReply(_ request: String) throws -> Data {
  let data = Data(nativeString(jam_config(request)).utf8)
  if let value = try JSONSerialization.jsonObject(with: data) as? [String: Any],
    let error = value["error"] as? String
  {
    throw NSError(domain: "Jam Settings", code: 1, userInfo: [NSLocalizedDescriptionKey: error])
  }
  return data
}
final class SettingsDraft: ObservableObject {
  @Published var config = JamConfig.defaults
  @Published var path = ""
  @Published var conflict = false
  @Published var error = ""
  @Published var loaded = false
  @Published var saving = false
  @Published var installed: Set<String> = []
  private var revision = ""
  private var timer: Timer?
  private var checking = false
  struct Snapshot: Decodable {
    let config: JamConfig
    let path: String
    let revision: String
  }
  init() {
    DispatchQueue.global(qos: .userInitiated).async { [weak self] in
      do {
        let snapshot = try JSONDecoder().decode(Snapshot.self, from: configReply("snapshot"))
        let executable =
          ProcessInfo.processInfo.environment["PATH", default: ""].components(separatedBy: ":") + [
            NSHomeDirectory() + "/.local/bin", "/opt/homebrew/bin", "/usr/local/bin",
          ]
        let installed = Set(
          ["claude", "codex"].filter { name in
            executable.contains { FileManager.default.isExecutableFile(atPath: $0 + "/" + name) }
          })
        DispatchQueue.main.async {
          guard let self else { return }
          self.config = snapshot.config
          self.path = snapshot.path
          self.revision = snapshot.revision
          self.installed = installed
          self.loaded = true
          self.timer = Timer.scheduledTimer(withTimeInterval: 0.3, repeats: true) { [weak self] _ in
            self?.check()
          }
        }
      } catch { DispatchQueue.main.async { self?.error = error.localizedDescription } }
    }
  }
  deinit { timer?.invalidate() }
  func check() {
    guard loaded, !conflict, !saving, !checking else { return }
    checking = true
    DispatchQueue.global(qos: .utility).async { [weak self] in
      let result =
        try? JSONSerialization.jsonObject(with: configReply("revision")) as? [String: String]
      DispatchQueue.main.async {
        guard let self else { return }
        self.checking = false
        if !self.saving && result?["revision"] != self.revision { self.conflict = true }
      }
    }
  }
  func save(_ done: @escaping () -> Void) {
    guard loaded, !saving, !conflict else { return }
    saving = true
    let draft = config
    let revision = revision
    DispatchQueue.global(qos: .userInitiated).async { [weak self] in
      do {
        let value = try JSONSerialization.jsonObject(with: JSONEncoder().encode(draft))
        let payload = try JSONSerialization.data(withJSONObject: [
          "config": value, "expected_revision": revision,
        ])
        _ = try configReply(String(decoding: payload, as: UTF8.self))
        DispatchQueue.main.async {
          self?.saving = false
          done()
        }
      } catch {
        DispatchQueue.main.async {
          self?.saving = false
          self?.error = error.localizedDescription
          if error.localizedDescription.contains("reopen") { self?.conflict = true }
        }
      }
    }
  }
}
extension Store {
  func openSettings() {
    NSApp.activate(ignoringOtherApps: true)
    showSettings = true
  }
}
struct AppSettingsView: View {
  @ObservedObject var store: Store
  let close: () -> Void
  @StateObject private var draft = SettingsDraft()
  @State private var section = "General"
  @State private var theme: JamTheme
  @State private var modelProvider = "claude"
  @State private var modelEndpoint = ""
  @State private var catalogs: [String: AgentModelCatalog] = [:]
  @State private var catalogErrors: [String: String] = [:]
  @State private var catalogLoading: Set<String> = []
  @State private var recording: String?
  @State private var recorder: Any?
  private let categories = [
    ("General", Image(systemName: "slider.horizontal.3")),
    ("Agents", Image(systemName: "person.2")),
    ("Coordination", Image(nsImage: CoordinatorMark.image).renderingMode(.template)),
    ("Integrations", Image(systemName: "puzzlepiece.extension")),
    ("Appearance", Image(systemName: "paintbrush")), ("Keyboard", Image(systemName: "keyboard")),
  ]
  init(store: Store, initialSection: String = "General", close: @escaping () -> Void) {
    self.store = store
    self.close = close
    _section = State(initialValue: initialSection)
    _theme = State(initialValue: store.theme)
  }
  var body: some View {
    let appearance = draft.loaded ? draft.config.appearance : store.config?.appearance
    HStack(spacing: 0) {
      sidebar
      VStack(alignment: .leading, spacing: 0) {
        Text(section).font(.title3.weight(.semibold))
          .padding(.horizontal, 22).padding(.top, 24).padding(.bottom, 20)
        if draft.conflict {
          Label(
            "Settings changed outside this window. Close and reopen Settings before saving.",
            systemImage: "exclamationmark.triangle"
          ).font(.callout).foregroundStyle(.orange)
            .padding(.horizontal, 22).padding(.bottom, 16)
        }
        if !draft.loaded && draft.error.isEmpty {
          ProgressView("Loading settings…").padding(.horizontal, 22)
        }
        if section == "Tasks and Agents" {
          CoordinationView().environmentObject(store)
        } else {
          ScrollView {
            VStack(alignment: .leading, spacing: 24) {
              settingsContent
            }.padding(.horizontal, 22).padding(.bottom, 20)
              .frame(maxWidth: .infinity, alignment: .leading)
          }.disabled(!draft.loaded || draft.saving)
        }
        footer
      }.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }.background(Color(nsColor: theme.background))
      .jamTheme(theme)
      .interactiveDismissDisabled(draft.saving)
      .preferredColorScheme(appearance == "dark" ? .dark : appearance == "light" ? .light : nil)
      .onAppear {
        recorder = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
          guard store.showSettings, event.window?.sheetParent != nil,
            event.window == NSApp.keyWindow, let id = recording
          else {
            return event
          }
          if event.keyCode == 53 {
            recording = nil
            return nil
          }
          var parts: [String] = []
          let flags = event.modifierFlags
          for (flag, name) in [
            (NSEvent.ModifierFlags.control, "ctrl"), (.shift, "shift"), (.option, "alt"),
            (.command, "cmd"),
          ] { if flags.contains(flag) { parts.append(name) } }
          guard parts.contains(where: { $0 != "shift" }),
            let text = event.charactersIgnoringModifiers?.lowercased(), text.count == 1
          else { return nil }
          draft.config.bindings[id] = [(parts + [text]).joined(separator: "+")]
          recording = nil
          return nil
        }
      }.onDisappear { if let recorder { NSEvent.removeMonitor(recorder) } }
  }
  private func categoryButton(_ title: String, icon: Image) -> some View {
    Button {
      recording = nil
      section = title
      if section == "Coordination" { loadModels() }
    } label: {
      Label {
        Text(title).font(.system(size: 13, weight: .medium))
      } icon: {
        icon.font(.system(size: 13)).frame(width: 20)
      }.frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 10).frame(height: 34)
        .contentShape(Rectangle())
        .foregroundStyle(section == title ? Color.white : Color.primary)
        .background(
          section == title ? Color(nsColor: theme.selectionFill) : Color.clear,
          in: RoundedRectangle(cornerRadius: 6))
    }.buttonStyle(.plain)
      .accessibilityAddTraits(section == title ? .isSelected : [])
  }
  private var sidebar: some View {
    VStack(alignment: .leading, spacing: 4) {
      ForEach(categories, id: \.0) { category in
        categoryButton(category.0, icon: category.1)
      }
      Spacer(minLength: 16)
      categoryButton("Tasks and Agents", icon: Image(systemName: "checklist"))
      Button {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: draft.path)])
      } label: {
        Label {
          Text("Settings File").font(.system(size: 12))
        } icon: {
          Image(systemName: "doc.text").font(.system(size: 13)).frame(width: 20)
        }.foregroundStyle(.secondary)
          .frame(maxWidth: .infinity, alignment: .leading)
          .padding(.horizontal, 10).frame(height: 32).contentShape(Rectangle())
      }.buttonStyle(.plain).disabled(draft.path.isEmpty).help(draft.path)
    }.padding(.horizontal, 8).padding(.top, 16).padding(.bottom, 8)
      .frame(width: 180, alignment: .leading)
      .frame(maxHeight: .infinity)
      .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 12))
      .padding(.leading, 8).padding(.vertical, 8)
  }
  private var footer: some View {
    VStack(alignment: .leading, spacing: 12) {
      if !draft.error.isEmpty {
        ErrorNotice(message: draft.error)
      }
      HStack {
        Spacer()
        if draft.saving { ProgressView().controlSize(.small) }
        Button(section == "Tasks and Agents" ? "Done" : "Cancel", action: close).keyboardShortcut(
          .cancelAction
        ).disabled(draft.saving)
        if section != "Tasks and Agents" {
          Button("Save") {
            draft.save {
              store.theme = theme
              store.reloadConfig()
              close()
            }
          }.keyboardShortcut(.defaultAction).buttonStyle(.borderedProminent)
            .tint(Color(nsColor: theme.selectionFill))
            .disabled(
              !draft.loaded || draft.conflict || draft.saving
                || WorktreePathPreview(template: draft.config.worktreesDirectory).error != nil)
        }
      }.controlSize(.regular)
    }.padding(.horizontal, 22).padding(.vertical, 16)
  }
  @ViewBuilder private var settingsContent: some View {
    switch section {
    case "General":
      settingsGroup("Updates") {
        AppUpdateSettings(
          controller: store.appUpgrade, automatic: $draft.config.automaticCheckUpdates)
      }
      settingsGroup("Workspaces") {
        settingToggle(
          "Close workspace after its last tab closes", isOn: $draft.config.closeEmptyWorkspaces)
        Divider()
        settingToggle("Close tab after its last pane closes", isOn: $draft.config.closeEmptyTabs)
        Divider()
        settingToggle("Allow deleting checkout files", isOn: $draft.config.allowWorkspaceDeletion)
      }
      settingsNote(
        "Closing a workspace preserves its files. The deletion setting controls Delete Checkout.")
      settingsGroup("Terminal") {
        settingToggle("Copy text on selection", isOn: $draft.config.copyOnSelect)
      }
      settingsGroup("Worktrees") {
        WorktreeTemplateEditor(template: $draft.config.worktreesDirectory)
      }
    case "Agents":
      settingsGroup("Enabled integrations") {
        ForEach(["claude", "codex"], id: \.self) { provider in
          if provider != "claude" { Divider() }
          HStack(spacing: 8) {
            AgentBrandIcon(provider: provider)
            Text(AgentBrand.name(provider))
            Spacer()
            Text(draft.installed.contains(provider) ? "Detected" : "Not found on this Mac")
              .font(.caption).foregroundStyle(.secondary)
            Toggle(
              AgentBrand.name(provider),
              isOn: Binding(
                get: { draft.config.enabledAgents.contains(provider) },
                set: { enabled in
                  draft.config.enabledAgents.removeAll { $0 == provider }
                  if enabled { draft.config.enabledAgents.append(provider) }
                }
              )
            ).labelsHidden().fixedSize()
          }
        }
      }
      settingsNote(
        "Applies to all connected machines. Disabled integrations hide usage and coordination. Terminal commands still work."
      )
    case "Coordination":
      settingsGroup("Team") {
        HStack {
          Text("Maximum workers")
          Spacer()
          Text("\(draft.config.maxAgentsPerCoordinator)").monospacedDigit()
          Toggle("Automatically resume interrupted work", isOn: $draft.config.resumeInterruptedWork)
          Stepper("Maximum workers", value: $draft.config.maxAgentsPerCoordinator, in: 1...64)
            .labelsHidden().fixedSize()
        }
      }
      settingsNote(
        "Shared by a workspace and its worktree coordinators. Lowering the limit leaves existing workers running."
      )
      modelSettings
    case "Integrations":
      settingsGroup("File previews") { PreviewIntegrations(store: store) }
    case "Appearance":
      settingsGroup("Theme") {
        Picker("Theme", selection: $theme) {
          ForEach(JamTheme.allCases) { theme in Text(theme.title).tag(theme) }
        }.pickerStyle(.segmented).labelsHidden().tint(Color(nsColor: theme.selectionFill))
      }
      settingsGroup("Color scheme") {
        Picker("Color scheme", selection: $draft.config.appearance) {
          Text("System").tag("system")
          Text("Light").tag("light")
          Text("Dark").tag("dark")
        }.pickerStyle(.segmented).labelsHidden().tint(Color(nsColor: theme.selectionFill))
      }
    default:
      settingsGroup("Shortcuts") {
        ForEach(JamCommand.all) { command in
          if command.id != JamCommand.all.first?.id { Divider() }
          HStack(spacing: 10) {
            Text(command.title).frame(width: 145, alignment: .leading)
            TextField(
              "Unassigned",
              text: Binding(
                get: { draft.config.bindings[command.id]?.joined(separator: ", ") ?? "" },
                set: {
                  draft.config.bindings[command.id] = $0.split(separator: ",").map {
                    $0.trimmingCharacters(in: .whitespaces).lowercased()
                  }
                }
              )
            ).font(.system(size: 12, design: .monospaced))
              .accessibilityLabel(command.title)
            Button(recording == command.id ? "Press keys…" : "Record") { recording = command.id }
            Button("Reset") {
              draft.config.bindings[command.id] = JamConfig.defaults.bindings[command.id] ?? []
            }
          }
        }
      }
    }
  }
  private var catalogKey: String { modelEndpoint + ":" + modelProvider }
  private func loadModels(refresh: Bool = false) {
    let provider = modelProvider
    let endpoint = modelEndpoint
    let key = endpoint + ":" + provider
    guard !catalogLoading.contains(key), refresh || catalogs[key] == nil else { return }
    catalogLoading.insert(key)
    catalogErrors.removeValue(forKey: key)
    DispatchQueue.global(qos: .utility).async {
      let capabilities = rpc("Capabilities", endpoint: endpoint)["Capabilities"] as? [String] ?? []
      let result: [String: Any] =
        capabilities.contains("coordination_models")
        ? rpc(["AgentModels": ["provider": provider, "refresh": refresh]], endpoint: endpoint)
        : ["error": "Update this machine's server to discover models."]
      let catalog = result["AgentModels"].flatMap {
        try? JSONSerialization.data(withJSONObject: $0)
      }
      .flatMap { try? JSONDecoder().decode(AgentModelCatalog.self, from: $0) }
      DispatchQueue.main.async {
        catalogLoading.remove(key)
        if let catalog {
          catalogs[key] = catalog
        } else {
          catalogErrors[key] = result["error"] as? String ?? "Could not discover models."
        }
      }
    }
  }
  private var modelSettings: some View {
    VStack(alignment: .leading, spacing: 16) {
      HStack {
        Picker("Agent", selection: $modelProvider) {
          Text("Claude").tag("claude")
          Text("Codex").tag("codex")
        }.pickerStyle(.segmented).labelsHidden().frame(width: 200)
          .onChange(of: modelProvider) { _, _ in loadModels() }
        Spacer()
        if !store.remotes.isEmpty {
          Picker("Models on", selection: $modelEndpoint) {
            Text("Local").tag("")
            ForEach(store.remotes) { remote in Text(remote.name).tag(remote.id) }
          }.fixedSize().onChange(of: modelEndpoint) { _, _ in loadModels() }
        }
        if catalogLoading.contains(catalogKey) { ProgressView().controlSize(.small) }
        Button {
          loadModels(refresh: true)
        } label: {
          Image(systemName: "arrow.clockwise").frame(width: 24, height: 24).contentShape(
            Rectangle())
        }.buttonStyle(.plain).help("Refresh available models")
          .disabled(catalogLoading.contains(catalogKey))
      }
      settingsGroup("Models") {
        ForEach(AgentModelSettings.fields.indices, id: \.self) { index in
          if index != 0 { Divider() }
          let (label, field) = AgentModelSettings.fields[index]
          let model = Binding<String>(
            get: {
              (draft.config.agentModels[modelProvider] ?? AgentModelSettings())[keyPath: field]
            },
            set: { value in
              var choices = draft.config.agentModels[modelProvider] ?? AgentModelSettings()
              choices[keyPath: field] = value
              draft.config.agentModels[modelProvider] = choices
            })
          let choices = draft.config.agentModels[modelProvider] ?? AgentModelSettings()
          let resolved = choices.effective(field, fallback: catalogs[catalogKey]?.defaultModel)
          HStack(spacing: 12) {
            Text(label).frame(width: 105, alignment: .leading)
            TextField("Jam default · \(resolved)", text: model)
              .font(.system(size: 12, design: .monospaced)).accessibilityLabel("\(label) model")
            Menu {
              Button("Jam default") { model.wrappedValue = "" }
              Divider()
              ForEach(catalogs[catalogKey]?.models ?? []) { entry in
                Button(entry.name == entry.id ? entry.id : "\(entry.name) · \(entry.id)") {
                  model.wrappedValue = entry.id
                }
              }
            } label: {
              Image(systemName: "chevron.down")
            }
            .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize().help(
              "Choose a discovered model")
          }

        }
      }
      if let error = catalogErrors[catalogKey] { ErrorNotice(message: error) }
    }
  }

  private func settingToggle(_ title: String, isOn: Binding<Bool>) -> some View {
    HStack {
      Text(title)
      Spacer(minLength: 16)
      Toggle(title, isOn: isOn).labelsHidden().fixedSize()
    }
  }
  private func settingsGroup<Content: View>(
    _ title: String,
    @ViewBuilder content: () -> Content
  ) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      Text(title).font(.system(size: 12, weight: .semibold))
      VStack(alignment: .leading, spacing: 12, content: content)
        .font(.system(size: 13)).controlSize(.small).toggleStyle(.switch)
        .padding(14).frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 10))
    }
  }
  private func settingsNote(_ text: String) -> some View {
    Text(text).font(.caption).foregroundStyle(.secondary)
      .fixedSize(horizontal: false, vertical: true).padding(.top, -14)
  }

}

extension JamConfig {
  var hostSettings: [String: Any] {
    [
      "worktrees_directory": worktreesDirectory,
      "resume_interrupted_work": resumeInterruptedWork,
      "enabled_agents": enabledAgents, "max_agents_per_coordinator": maxAgentsPerCoordinator,
      "agent_models": agentModels.mapValues { choices in
        [
          "coordinator": choices.coordinator, "worker": choices.worker,
          "reviewer": choices.reviewer,
          "complex_work": choices.complexWork,
        ]
      },
      "close_empty_workspaces": closeEmptyWorkspaces, "close_empty_tabs": closeEmptyTabs,
      "allow_workspace_deletion": allowWorkspaceDeletion,
    ]
  }
}
// Cache per daemon incarnation: normal state polling does not need extra remote round trips.
private final class HostSettingsCache: @unchecked Sendable {
  static let shared = HostSettingsCache()
  let lock = NSLock()
  var entries: [String: (fingerprint: Data, at: Date, providers: Set<String>)] = [:]
}
// Host settings are an app-owned policy, separate from a remote user's config file.
func configureHost(_ settings: [String: Any]?, endpoint: String, incarnation: String? = nil) -> Set<
  String
>? {
  guard let settings,
    let fingerprint = try? JSONSerialization.data(withJSONObject: settings, options: .sortedKeys)
  else { return nil }
  let key = endpoint + ":" + (incarnation ?? "")
  let cache = HostSettingsCache.shared
  cache.lock.lock()
  let previous = cache.entries[key]
  cache.lock.unlock()
  if let previous, previous.fingerprint == fingerprint, Date().timeIntervalSince(previous.at) < 10 {
    return previous.providers
  }
  let capabilities = rpc("Capabilities", endpoint: endpoint)["Capabilities"] as? [String] ?? []
  guard capabilities.contains("app_settings_v1") else { return nil }
  var policy = settings
  if !capabilities.contains("agent_recovery_v1") {
    policy.removeValue(forKey: "resume_interrupted_work")
  }
  if !capabilities.contains("coordination_models") { policy.removeValue(forKey: "agent_models") }
  if !capabilities.contains("worktree_templates") {
    policy.removeValue(forKey: "worktrees_directory")
  }
  let result = rpc(["Configure": ["settings": policy]], endpoint: endpoint)
  guard let data = result["Settings"] as? [String: Any],
    let providers = data["providers"] as? [String: Bool]
  else { return nil }
  let detected = Set(providers.filter { $0.value }.map(\.key))
  cache.lock.lock()
  cache.entries[key] = (fingerprint, Date(), detected)
  cache.lock.unlock()
  return detected
}
