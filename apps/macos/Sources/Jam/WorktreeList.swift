import AppKit
import SwiftUI

/// Fixed-height native rows keep selection and scrolling independent of SwiftUI
/// layout and of the Store's background remote/state updates.
struct WorktreeList: NSViewRepresentable {
  let checkouts: [WorktreeCheckout]
  let opened: Set<String>
  @Binding var selection: String?
  var enabled = true
  let onOpen: () -> Void

  func makeCoordinator() -> Coordinator { Coordinator() }
  func makeNSView(context: Context) -> NSScrollView {
    let scroll = WorktreeScrollView()
    scroll.drawsBackground = false
    scroll.hasVerticalScroller = true
    scroll.autohidesScrollers = true
    scroll.horizontalScrollElasticity = .none
    let table = WorktreeTable()
    table.coordinator = context.coordinator
    table.headerView = nil
    table.backgroundColor = .clear
    table.focusRingType = .none
    table.style = .plain
    table.rowHeight = 50
    table.intercellSpacing = NSSize(width: 0, height: 3)
    table.usesAutomaticRowHeights = false
    table.allowsEmptySelection = true
    table.allowsMultipleSelection = false
    table.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
    let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("checkout"))
    column.resizingMask = .autoresizingMask
    table.addTableColumn(column)
    table.delegate = context.coordinator
    table.dataSource = context.coordinator
    table.target = context.coordinator
    table.doubleAction = #selector(Coordinator.openSelected)
    table.setAccessibilityLabel("Worktrees")
    scroll.documentView = table
    context.coordinator.table = table
    updateNSView(scroll, context: context)
    return scroll
  }

  func updateNSView(_ scroll: NSScrollView, context: Context) {
    let coordinator = context.coordinator
    coordinator.onSelect = { selection = $0 }
    coordinator.onOpen = onOpen
    coordinator.enabled = enabled
    coordinator.update(checkouts: checkouts, opened: opened, selection: selection)
  }

  final class Coordinator: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    weak var table: WorktreeTable?
    private(set) var checkouts: [WorktreeCheckout] = []
    private var opened = Set<String>()
    private var unavailable = Set<String>()
    private var updating = false
    private(set) var reloadCount = 0
    var enabled = true
    var onSelect: ((String?) -> Void)?
    var onOpen: (() -> Void)?

    func update(checkouts: [WorktreeCheckout], opened: Set<String>, selection: String?) {
      guard let table else { return }
      updating = true
      defer { updating = false }
      if self.checkouts != checkouts || self.opened != opened {
        self.checkouts = checkouts
        self.opened = opened
        unavailable = Set(
          checkouts.filter {
            opened.contains(WorktreeChoices.normalized($0.path))
          }.map(\.id))
        reloadCount += 1
        table.reloadData()
      }
      let index = checkouts.firstIndex { $0.id == selection && !unavailable.contains($0.id) }
      if let index {
        if table.selectedRow != index {
          table.selectRowIndexes(IndexSet(integer: index), byExtendingSelection: false)
        }
        // A selection inside the viewport must not move the scroll position.
        if !table.visibleRect.contains(table.rect(ofRow: index)) { table.scrollRowToVisible(index) }
      } else if table.selectedRow != -1 {
        table.deselectAll(nil)
      }
    }

    func numberOfRows(in tableView: NSTableView) -> Int { checkouts.count }
    func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool {
      enabled && checkouts.indices.contains(row) && !unavailable.contains(checkouts[row].id)
    }
    func tableViewSelectionDidChange(_ notification: Notification) {
      guard !updating, let table else { return }
      onSelect?(
        checkouts.indices.contains(table.selectedRow) ? checkouts[table.selectedRow].id : nil)
    }
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int)
      -> NSView?
    {
      let identifier = NSUserInterfaceItemIdentifier("checkout-cell")
      let cell =
        tableView.makeView(withIdentifier: identifier, owner: nil) as? WorktreeListCell
        ?? WorktreeListCell()
      cell.identifier = identifier
      cell.configure(checkouts[row], opened: unavailable.contains(checkouts[row].id))
      return cell
    }
    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
      WorktreeListRow()
    }
    func move(by direction: Int) {
      guard enabled, let table else { return }
      let selectable = checkouts.indices.filter { !unavailable.contains(checkouts[$0].id) }
      guard !selectable.isEmpty else { return }
      let position = selectable.firstIndex(of: table.selectedRow)
      let next =
        position.map { min(selectable.count - 1, max(0, $0 + direction)) }
        ?? (direction < 0 ? selectable.count - 1 : 0)
      table.selectRowIndexes(IndexSet(integer: selectable[next]), byExtendingSelection: false)
      if !table.visibleRect.contains(table.rect(ofRow: selectable[next])) {
        table.scrollRowToVisible(selectable[next])
      }
    }
    @objc func openSelected() {
      guard enabled, let table, table.selectedRow >= 0,
        tableView(table, shouldSelectRow: table.selectedRow)
      else { return }
      onOpen?()
    }
  }
}

final class WorktreeTable: NSTableView {
  weak var coordinator: WorktreeList.Coordinator?
  override func keyDown(with event: NSEvent) {
    if event.modifierFlags.intersection([.command, .control, .option]).isEmpty {
      switch event.keyCode {
      case 125:
        coordinator?.move(by: 1)
        return
      case 126:
        coordinator?.move(by: -1)
        return
      case 36, 76:
        coordinator?.openSelected()
        return
      default: break
      }
    }
    super.keyDown(with: event)
  }
}

private final class WorktreeListRow: NSTableRowView {
  override func drawSelection(in dirtyRect: NSRect) {
    NSColor.controlAccentColor.withAlphaComponent(0.15).setFill()
    NSBezierPath(roundedRect: bounds.insetBy(dx: 0, dy: 1), xRadius: 7, yRadius: 7).fill()
  }
}

private final class WorktreeListCell: NSView {
  private let icon = NSImageView()
  private let branch = NSTextField(labelWithString: "")
  private let path = NSTextField(labelWithString: "")
  private let status = NSTextField(labelWithString: "Open")
  override init(frame: NSRect) {
    super.init(frame: frame)
    icon.contentTintColor = .secondaryLabelColor
    branch.font = .systemFont(ofSize: 12, weight: .medium)
    path.font = .systemFont(ofSize: 10)
    path.textColor = .secondaryLabelColor
    status.font = .systemFont(ofSize: 10)
    status.textColor = .secondaryLabelColor
    for field in [branch, path, status] {
      field.maximumNumberOfLines = 1
      field.lineBreakMode = field === path ? .byTruncatingMiddle : .byTruncatingTail
      addSubview(field)
    }
    addSubview(icon)
  }
  required init?(coder: NSCoder) { fatalError("unsupported") }
  func configure(_ checkout: WorktreeCheckout, opened: Bool) {
    icon.image = NSImage(
      systemSymbolName: checkout.main ? "folder" : "arrow.triangle.branch",
      accessibilityDescription: nil)
    branch.stringValue = checkout.branch + (checkout.main ? "   Main" : "")
    path.stringValue = checkout.path
    status.isHidden = !opened
    alphaValue = opened ? 0.55 : 1
    toolTip = opened ? "Already open in this workspace" : checkout.path
    setAccessibilityLabel("\(checkout.branch), \(checkout.path)\(opened ? ", already open" : "")")
    needsLayout = true
  }
  override func layout() {
    super.layout()
    icon.frame = NSRect(x: 12, y: (bounds.height - 16) / 2, width: 16, height: 16)
    let statusWidth: CGFloat = status.isHidden ? 0 : 36
    let width = max(0, bounds.width - 50 - statusWidth)
    branch.frame = NSRect(x: 38, y: bounds.height - 23, width: width, height: 16)
    path.frame = NSRect(x: 38, y: bounds.height - 39, width: width, height: 13)
    status.frame = NSRect(
      x: max(0, bounds.width - 40), y: (bounds.height - 14) / 2, width: 32, height: 14)
  }
}

private final class WorktreeScrollView: NSScrollView {
  override func layout() {
    super.layout()
    guard let table = documentView as? NSTableView else { return }
    let width = contentSize.width
    if table.frame.width != width {
      table.setFrameSize(NSSize(width: width, height: table.frame.height))
    }
    if let column = table.tableColumns.first, column.width != width { column.width = width }
  }
}
