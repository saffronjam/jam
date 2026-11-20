import AppKit
import SwiftUI

struct WorkspaceTint {
  let name: String
  let hex: String
  static let presets = [
    WorkspaceTint(name: "Red", hex: "#E46B6B"),
    WorkspaceTint(name: "Orange", hex: "#EAA45D"),
    WorkspaceTint(name: "Yellow", hex: "#D7BE68"),
    WorkspaceTint(name: "Green", hex: "#7FB88A"),
    WorkspaceTint(name: "Teal", hex: "#71B9B4"),
    WorkspaceTint(name: "Blue", hex: "#6EA1E6"),
    WorkspaceTint(name: "Purple", hex: "#B08BDB"),
    WorkspaceTint(name: "Pink", hex: "#D68CB4"),
  ]
  static func color(_ hex: String) -> NSColor? {
    guard hex.count == 7, hex.first == "#", let value = UInt32(hex.dropFirst(), radix: 16) else {
      return nil
    }
    return NSColor(
      srgbRed: CGFloat((value >> 16) & 255) / 255,
      green: CGFloat((value >> 8) & 255) / 255, blue: CGFloat(value & 255) / 255, alpha: 1)
  }
  static func hex(_ color: NSColor) -> String? {
    guard let rgb = color.usingColorSpace(.sRGB) else { return nil }
    return String(
      format: "#%02X%02X%02X", Int((rgb.redComponent * 255).rounded()),
      Int((rgb.greenComponent * 255).rounded()), Int((rgb.blueComponent * 255).rounded()))
  }
  static func swatch(_ color: NSColor) -> NSImage {
    let image = NSImage(size: NSSize(width: 14, height: 14))
    image.lockFocus()
    color.setFill()
    NSBezierPath(ovalIn: NSRect(x: 2, y: 2, width: 10, height: 10)).fill()
    image.unlockFocus()
    return image
  }
}

extension Store {
  func tint(for workspace: String?) -> NSColor? {
    guard let workspace, let value = workspaceTints[workspace] else { return nil }
    return WorkspaceTint.color(value)
  }
  func setTint(_ hex: String?, for workspace: String) {
    if let hex, WorkspaceTint.color(hex) == nil { return }
    workspaceTints[workspace] = hex
  }
  func tintMenu(for workspace: String) -> NSMenu {
    let menu = NSMenu(title: "Tint")
    menu.allowsContextMenuPlugIns = false
    let reset = SidebarMenuItem(title: "Default") { self.setTint(nil, for: workspace) }
    reset.state = workspaceTints[workspace] == nil ? .on : .off
    menu.addItem(reset)
    for preset in WorkspaceTint.presets {
      let item = SidebarMenuItem(title: preset.name) { self.setTint(preset.hex, for: workspace) }
      item.image = WorkspaceTint.swatch(WorkspaceTint.color(preset.hex)!)
      item.state = workspaceTints[workspace] == preset.hex ? .on : .off
      menu.addItem(item)
    }
    menu.addItem(.separator())
    menu.addItem(
      SidebarMenuItem(title: "Custom…") {
        if self.tintPicker == nil { self.tintPicker = WorkspaceTintPicker(store: self) }
        self.tintPicker?.show(workspace: workspace)
      })
    return menu
  }
}

final class WorkspaceTintPicker: NSObject {
  weak var store: Store?
  var workspace: String?
  init(store: Store) { self.store = store }
  func show(workspace: String) {
    self.workspace = workspace
    let panel = NSColorPanel.shared
    panel.setTarget(nil)
    panel.color = store?.tint(for: workspace) ?? .controlAccentColor
    panel.showsAlpha = false
    panel.isContinuous = true
    panel.setTarget(self)
    panel.setAction(#selector(changed(_:)))
    NSApp.activate()
    panel.makeKeyAndOrderFront(nil)
  }
  @objc func changed(_ panel: NSColorPanel) {
    guard let workspace, let hex = WorkspaceTint.hex(panel.color) else { return }
    store?.setTint(hex, for: workspace)
  }
}
