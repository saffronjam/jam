import CJam
import SwiftUI

struct AgentIntegration: Decodable, Identifiable {
  enum CodingKeys: String, CodingKey {
    case agent, detected, skill, hook
    case skillPath = "skill_path"
    case hookPath = "hook_path"
  }
  let agent: String
  let detected: Bool
  let skillPath: String
  let hookPath: String
  let skill: String
  let hook: String
  var id: String { agent }
  var installed: Bool { skill == "current" && hook == "current" }
}

struct AgentIntegrations: View {
  @Environment(\.dismiss) var dismiss
  @State private var agents: [AgentIntegration] = []
  @State private var busy = false
  @State private var error = ""
  @State private var updated = false

  private func run(_ action: String, agent: String? = nil) {
    busy = true
    error = ""
    DispatchQueue.global().async {
      func call(_ action: String) -> String {
        var request: [String: Any] = ["action": action]
        if let agent, action != "status" { request["agents"] = [agent] }
        guard let data = try? JSONSerialization.data(withJSONObject: request) else {
          return "{\"error\":\"Could not encode integration request\"}"
        }
        return nativeString(jam_agent_integrations(String(decoding: data, as: UTF8.self)))
      }
      let result = call(action)
      let message =
        (try? JSONSerialization.jsonObject(with: Data(result.utf8)) as? [String: Any])?["error"]
        as? String
      let status = action == "status" || message != nil ? result : call("status")
      let statusMessage =
        (try? JSONSerialization.jsonObject(with: Data(status.utf8)) as? [String: Any])?["error"]
        as? String
      let entries = try? JSONDecoder().decode([AgentIntegration].self, from: Data(status.utf8))
      DispatchQueue.main.async {
        busy = false
        if let message = message ?? statusMessage { error = message }
        if let entries { agents = entries }
        if action != "status" && message == nil { updated = true }
      }
    }
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      Text("Agent Integrations").font(.headline)
      Text(
        "Install a Jam skill and startup hook on this Mac. Agents learn their Jam context without a message from you. Hooks do nothing outside Jam."
      )
      .font(.callout).foregroundStyle(.secondary)
      ForEach(agents) { agent in
        VStack(alignment: .leading, spacing: 6) {
          HStack {
            Text(agent.agent == "claude" ? "Claude Code" : "Codex").fontWeight(.medium)
            if !agent.detected { Text("Not detected").font(.caption).foregroundStyle(.secondary) }
            Spacer()
            Text(agent.installed ? "Installed" : "Skill: \(agent.skill), hook: \(agent.hook)")
              .font(.caption).foregroundStyle(.secondary)
          }
          Text(agent.skillPath).lineLimit(1).truncationMode(.head).help(agent.skillPath)
          Text(agent.hookPath).lineLimit(1).truncationMode(.head).help(agent.hookPath)
          HStack {
            Spacer()
            if agent.hook != "missing" || agent.skill == "current" || agent.skill == "outdated" {
              Button("Remove") { run("remove", agent: agent.agent) }
                .accessibilityIdentifier("integration.\(agent.agent).remove")
            }
            if !agent.installed {
              Button(agent.skill == "outdated" || agent.hook == "outdated" ? "Update" : "Install") {
                run("setup", agent: agent.agent)
              }.disabled(agent.skill == "foreign")
                .accessibilityIdentifier("integration.\(agent.agent).install")
            }
          }
          if agent.skill == "foreign" {
            Text("An existing Jam skill is not managed by this installer and will be preserved.")
              .foregroundStyle(.secondary)
          }
        }.font(.caption)
        Divider()
      }
      if updated {
        Text(
          "Start a new agent session. In Codex, approve the hook with /hooks. Claude Code requires a trusted folder."
        )
        .font(.callout).foregroundStyle(.secondary)
      }
      if !error.isEmpty { ErrorNotice(message: error) }
      HStack {
        if busy { ProgressView().controlSize(.small) }
        Spacer()
        Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
      }
    }.padding(24).frame(width: 500).disabled(busy).onAppear { run("status") }
  }
}
