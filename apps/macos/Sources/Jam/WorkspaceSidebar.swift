import AppKit
import QuartzCore
import SwiftUI

struct WorkspaceSidebar: NSViewRepresentable {
  @ObservedObject var store: Store
  func makeCoordinator() -> Coordinator { Coordinator(store) }
  func makeNSView(context: Context) -> NSScrollView {
    let scroll = WorkspaceScrollView(frame: .zero)
    scroll.contentView = WorkspaceClipView()
    scroll.contentView.drawsBackground = false
    scroll.horizontalScrollElasticity = .none
    let outline = WorkspaceOutline()
    let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("workspace"))
    outline.addTableColumn(column)
    outline.outlineTableColumn = column
    outline.columnAutoresizingStyle = .lastColumnOnlyAutoresizingStyle
    column.resizingMask = .autoresizingMask
    column.minWidth = 60
    outline.headerView = nil
    outline.style = .plain
    outline.backgroundColor = .clear
    outline.rowSizeStyle = .custom
    outline.intercellSpacing = NSSize(width: 0, height: 3)
    outline.indentationPerLevel = 20
    outline.focusRingType = .none
    outline.dataSource = context.coordinator
    outline.delegate = context.coordinator
    outline.coordinator = context.coordinator
    context.coordinator.outline = outline
    scroll.documentView = outline
    scroll.drawsBackground = false
    updateNSView(scroll, context: context)
    return scroll
  }
  func updateNSView(_ view: NSScrollView, context: Context) {
    guard let outline = view.documentView as? WorkspaceOutline else { return }
    context.coordinator.store = store
    context.coordinator.update(outline)
    if context.coordinator.lastTheme != store.theme {
      context.coordinator.lastTheme = store.theme
      for case let editor as InlineNameEditor in outline.subviews { editor.theme = store.theme }
      outline.enumerateAvailableRowViews { row, _ in row.needsDisplay = true }
    }
  }
  final class Item {
    let key: String
    var machine: String?
    var repositoryGroup = false
    var isCoordinator = false
    var loading = false
    var connectionAttempt = false
    var offline = false
    var machineLabel: String?
    var topInset: CGFloat = 0
    var workspace: Workspace?
    var path: String
    var children: [Item] = []
    var tintID: String? { isCoordinator ? workspace?.coordinator?.id : workspace?.id }
    var displayName: String {
      if isCoordinator { return "Coordinator" }
      if machine != nil { return path }
      return workspace?.name ?? (path as NSString).lastPathComponent
    }
    init(key: String, workspace: Workspace?, path: String) {
      self.key = key
      self.workspace = workspace
      self.path = path
    }
  }
  final class Coordinator: NSObject, NSOutlineViewDataSource, NSOutlineViewDelegate {
    var store: Store
    weak var outline: NSOutlineView?
    var roots: [Item] = []
    var items: [String: Item] = [:]
    var signature = ""
    var rowSignatures: [String: [String]] = [:]
    var rowHeights: [String: CGFloat] = [:]
    var updating = false
    var lastSelected = ""
    var lastTheme: JamTheme?
    var editing: String?
    init(_ store: Store) { self.store = store }
    var activeItems: [Item] {
      func flatten(_ item: Item) -> [Item] { [item] + item.children.flatMap(flatten) }
      return roots.flatMap(flatten)
    }
    func parent(of item: Item) -> Item? {
      activeItems.first { $0.children.contains { $0 === item } }
    }
    private func item(key: String, workspace: Workspace?, path: String) -> Item {
      if let existing = items[key] { return existing }
      if let workspace, let origin = store.workspaceCreationOrigins[workspace.id],
        let pending = items.removeValue(forKey: origin)
          ?? items.removeValue(
            forKey: WorkspaceSelection.endpoint(of: origin) + ":" + origin)
      {
        // Preserve the native row and its stable key when its server ID arrives.
        items[key] = pending
        return pending
      }
      let item = Item(key: key, workspace: workspace, path: path)
      items[key] = item
      return item
    }
    private func workspaceSignature(_ workspace: Workspace) -> String {
      let coordinator = workspace.coordinator
      let context = workspace.context
      let summary = store.gitSummaries[workspace.id]
      let fields: [String] = [
        workspace.id, store.workspaceCreations[workspace.id]?.failure ?? "", workspace.parent ?? "",
        workspace.repository ?? "", workspace.name,
        store.workspaceRemovals[workspace.id].map { "\($0.action.label) \($0.showsSpinner)" } ?? "",
        coordinator?.id ?? "", coordinator?.pane ?? "", workspace.sidebarBranch ?? "",
        context?.path ?? "", context?.repository ?? "", String(context?.ready ?? false),
        context?.createDisabledReason ?? "", store.workspaceTints[workspace.id] ?? "",
        store.workspaceTints[coordinator?.id ?? ""] ?? "", String(summary?.added ?? 0),
        String(summary?.removed ?? 0), summary?.review?.displaySignature ?? "",
      ]
      return fields.joined(separator: "|")
    }
    private func rowSignature(for item: Item) -> (String, [String]) {
      let workspace = item.workspace
      let parentItem = parent(of: item)
      let summary: GitSummary? =
        item.isCoordinator ? nil : workspace.flatMap { store.gitSummaries[$0.id] }
      let endpoint: String =
        workspace.map { WorkspaceSelection.endpoint(of: $0.id) } ?? item.machine ?? ""
      let branch: String = item.isCoordinator ? "" : (workspace?.sidebarBranch ?? "")
      let fields: [String] = [
        item.path, item.machineLabel ?? "", String(item.loading), String(item.connectionAttempt),
        String(item.offline),
        "\(item.topInset)", workspace?.name ?? "", branch, String(!item.children.isEmpty),
        String(summary?.added ?? 0), String(summary?.removed ?? 0),
        summary?.review?.displaySignature ?? "",
        store.workspaceTints[item.tintID ?? ""] ?? "", parentItem?.key ?? "",
        store.workspaceTints[parentItem?.workspace?.id ?? ""] ?? "",
        iconLoadingLabel(for: item) ?? "",
        store.remoteMessages[endpoint] ?? "",
      ]
      return (item.key, fields)
    }
    private func iconLoadingLabel(for item: Item) -> String? {
      guard !item.isCoordinator, let id = item.workspace?.id else { return nil }
      if let removal = store.workspaceRemovals[id], removal.showsSpinner {
        return removal.action.label
      }
      if let creation = store.workspaceCreations[id], creation.failure == nil {
        return creation.action.opensExisting ? "Opening workspace" : "Creating workspace"
      }
      return nil
    }
    func update(_ outline: NSOutlineView) {
      guard editing == nil, !store.reorderingWorkspaces else { return }
      let remoteSignatures: [String] = store.remotes.map { remote -> String in
        let message: String = store.remoteMessages[remote.id] ?? ""
        let state: String =
          "\(store.remoteSidebarState(remote.id))|\(store.remoteConnectionState(remote.id))"
        let presentation = store.machinePresentation(remote.id)
        return [
          remote.id, remote.name, message, state, presentation.label ?? "",
          String(presentation.isAttempting), String(presentation.warning),
        ].joined(separator: "|")
      }
      let workspaceSignatures: [String] = store.displayedWorkspaces.map(workspaceSignature)
      let paneSignatures: [String] = store.panes.values.map { pane -> String in
        "\(pane.id):\(pane.status):\(pane.exited):\(pane.agentProvider ?? ""):\(pane.agentRole ?? "")"
      }
      let next: String = [
        "\(store.groupsMachines)|", remoteSignatures.joined(),
        String(describing: store.machinePresentation("")),
        workspaceSignatures.joined(separator: ";"), paneSignatures.sorted().joined(separator: ";"),
      ].joined()
      updating = true
      defer { updating = false }
      if signature != next {
        let previousKeys = Set(activeItems.map(\.key))
        let previousRoots = roots
        let previousChildren = Dictionary(
          uniqueKeysWithValues: activeItems.map { ($0.key, $0.children) })
        let expanded = Set(activeItems.filter { outline.isItemExpanded($0) }.map(\.key))
        let initial = signature.isEmpty
        signature = next
        roots = []
        let workspaces = store.displayedWorkspaces
        for workspace in workspaces
        where workspace.parent == nil || !workspaces.contains(where: { $0.id == workspace.parent })
        {
          let endpoint = WorkspaceSelection.endpoint(of: workspace.id)
          let root = item(
            key: endpoint + ":" + workspace.id, workspace: workspace, path: workspace.path)
          root.workspace = workspace
          root.path = workspace.path
          var repositories: [String: [Workspace]] = [:]
          var order: [String] = []
          for child in workspaces where child.parent == workspace.id {
            let repository = child.repository ?? child.path
            if repositories[repository] == nil { order.append(repository) }
            repositories[repository, default: []].append(child)
          }
          func coordinatorRows(_ owner: Workspace) -> [Item] {
            guard owner.hasCoordinator else { return [] }
            let row = item(key: "coordinator:" + owner.id, workspace: owner, path: owner.path)
            row.isCoordinator = true
            row.workspace = owner
            row.path = owner.path
            row.children = []
            return [row]
          }
          func checkoutRows(_ children: [Workspace]) -> [Item] {
            children.map { child in
              let row = item(key: child.id, workspace: child, path: child.path)
              row.workspace = child
              row.path = child.path
              row.children = coordinatorRows(child)
              return row
            }
          }
          root.children =
            order.count <= 1
            ? checkoutRows(order.first.flatMap { repositories[$0] } ?? [])
            : order.map { repository in
              let children = repositories[repository]!
              let path = children[0].mainPath ?? children[0].path
              let group = item(
                key: "repository:" + workspace.id + ":" + repository, workspace: nil, path: path)
              group.repositoryGroup = true
              group.path = path
              group.children = checkoutRows(children)
              return group
            }
          root.children.insert(contentsOf: coordinatorRows(workspace), at: 0)
          roots.append(root)
        }
        let workspaceGroups = roots
        if store.groupsMachines {
          roots = []
          for endpoint in [""] + store.remotes.map(\.id) {
            let key = "machine:" + endpoint
            let label =
              endpoint.isEmpty
              ? "Local" : store.remotes.first { $0.id == endpoint }?.name ?? endpoint
            let machine = items[key] ?? Item(key: key, workspace: nil, path: label)
            machine.machine = endpoint
            let presentation = store.machinePresentation(endpoint)
            machine.machineLabel = presentation.label
            let state =
              endpoint.isEmpty ? RemoteSidebarState.ready : store.remoteSidebarState(endpoint)
            machine.loading = state == .loading
            machine.connectionAttempt = presentation.isAttempting
            machine.offline = presentation.label == "Offline"
            machine.topInset = roots.isEmpty ? 0 : 12
            machine.path = label
            machine.children =
              state != .ready
              ? []
              : workspaceGroups.filter {
                let id = $0.workspace?.id ?? $0.children.first?.workspace?.id ?? ""
                return endpoint.isEmpty ? !id.contains("|") : id.hasPrefix(endpoint + "|")
              }
            items[key] = machine
            roots.append(machine)
          }
        }
        let desiredRoots = roots
        let desiredItems = activeItems
        let desiredChildren = Dictionary(
          uniqueKeysWithValues: desiredItems.map { ($0.key, $0.children) })
        let rowEntries: [(String, [String])] = desiredItems.map { rowSignature(for: $0) }
        let nextRowSignatures: [String: [String]] = Dictionary(uniqueKeysWithValues: rowEntries)
        let heightEntries: [(String, CGFloat)] = desiredItems.map { item -> (String, CGFloat) in
          let height: CGFloat =
            item.repositoryGroup
            ? 26
            : SidebarGeometry.rowHeight(
              hasBranch: !item.isCoordinator && item.workspace?.sidebarBranch != nil,
              isMachine: item.machine != nil)
          return (item.key, height + item.topInset)
        }
        let nextRowHeights: [String: CGFloat] = Dictionary(uniqueKeysWithValues: heightEntries)
        if initial {
          outline.reloadData()
        } else {
          // Restore the old model, then apply matching AppKit edits. reloadData()
          // discards every row and cannot animate a remote group appearing.
          roots = previousRoots
          for item in desiredItems { item.children = previousChildren[item.key] ?? [] }
          NSAnimationContext.runAnimationGroup { context in
            context.duration = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 0 : 0.22
            context.timingFunction = CAMediaTimingFunction(controlPoints: 0.22, 1, 0.36, 1)
            outline.beginUpdates()
            reconcile(desiredRoots, children: desiredChildren, parent: nil, outline: outline)
            outline.endUpdates()
            for item in desiredItems
            where !item.children.isEmpty
              && (expanded.contains(item.key) || !previousKeys.contains(item.key)
                || previousChildren[item.key]?.isEmpty == true)
            { outline.animator().expandItem(item) }
          }
          // Git metadata can change a row from one line to two after restart.
          // Resolve its new height before creating the replacement cell. Metadata
          // updates must not inherit the insertion/reordering animation above.
          NSAnimationContext.runAnimationGroup { context in
            context.duration = 0
            context.allowsImplicitAnimation = false
            let changed = desiredItems.filter {
              previousKeys.contains($0.key) && rowSignatures[$0.key] != nextRowSignatures[$0.key]
            }
            let resized = IndexSet(
              changed.compactMap { item -> Int? in
                let row = outline.row(forItem: item)
                return row >= 0 && rowHeights[item.key] != nextRowHeights[item.key] ? row : nil
              })
            if !resized.isEmpty { outline.noteHeightOfRows(withIndexesChanged: resized) }
            for item in changed { outline.reloadItem(item) }
          }
        }
        if initial {
          for item in desiredItems where !item.children.isEmpty { outline.expandItem(item) }
        }
        rowSignatures = nextRowSignatures
        rowHeights = nextRowHeights
        for row in 0..<outline.numberOfRows {
          if let view = outline.rowView(atRow: row, makeIfNecessary: false) as? WorkspaceRow,
            let item = outline.item(atRow: row) as? Item
          {
            configure(view, for: item)
            view.needsDisplay = true
          }
        }
      }
      let selectionKey =
        store.selected.isEmpty
        ? "machine:" + store.currentEndpoint : "\(store.selected):\(store.surface)"
      if lastSelected != selectionKey {
        if let selectedItem = items.values.first(where: {
          $0.workspace?.id == store.selected && !store.selected.isEmpty
            && $0.isCoordinator == (store.surface == .coordinator)
        }) {
          var child = selectedItem
          while let parent = self.parent(of: child) {
            outline.expandItem(parent)
            child = parent
          }
          if store.workspaceCreations[store.selected] != nil {
            let row = outline.row(forItem: selectedItem)
            if row >= 0 { outline.scrollRowToVisible(row) }
          }
        }
        lastSelected = selectionKey
      }
      if let row = (0..<outline.numberOfRows).first(where: {
        guard let item = outline.item(atRow: $0) as? Item else { return false }
        return !store.selected.isEmpty && item.workspace?.id == store.selected
          && item.isCoordinator == (store.surface == .coordinator)
      }) {
        outline.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
      } else {
        outline.deselectAll(nil)
      }
    }
    private func reconcile(
      _ desired: [Item], children: [String: [Item]], parent: Item?, outline: NSOutlineView
    ) {
      var current = parent?.children ?? roots
      for edit in SidebarEdit.between(current.map(\.key), desired.map(\.key)) {
        switch edit {
        case .remove(let index):
          current.remove(at: index)
        case .insert(let index):
          current.insert(desired[index], at: index)
        case .move(let from, let to):
          current.insert(current.remove(at: from), at: to)
        }
        if let parent { parent.children = current } else { roots = current }
        switch edit {
        case .remove(let index):
          outline.removeItems(
            at: IndexSet(integer: index), inParent: parent, withAnimation: [.effectFade, .slideUp])
        case .insert(let index):
          outline.insertItems(
            at: IndexSet(integer: index), inParent: parent,
            withAnimation: [.effectFade, .slideDown])
        case .move(let from, let to):
          let offset = parent?.children.filter(\.isCoordinator).count ?? 0
          outline.moveItem(at: from + offset, inParent: parent, to: to + offset, inParent: parent)
        }
      }
      for item in desired {
        reconcile(children[item.key] ?? [], children: children, parent: item, outline: outline)
      }
    }
    func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
      (item as? Item)?.children.count ?? roots.count
    }
    func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
      (item as? Item)?.children[index] ?? roots[index]
    }
    func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
      let item = item as! Item
      return !item.loading && !item.children.isEmpty
    }
    func outlineView(_ outlineView: NSOutlineView, shouldSelectItem item: Any) -> Bool {
      (item as? Item)?.workspace != nil
    }
    func outlineView(_ outlineView: NSOutlineView, heightOfRowByItem item: Any) -> CGFloat {
      let item = item as! Item
      return
        (item.repositoryGroup
        ? 26
        : SidebarGeometry.rowHeight(
          hasBranch: !item.isCoordinator && item.workspace?.sidebarBranch != nil,
          isMachine: item.machine != nil))
        + item.topInset
    }
    func outlineView(_ outlineView: NSOutlineView, rowViewForItem item: Any) -> NSTableRowView? {
      let row = WorkspaceRow()
      let item = item as! Item
      configure(row, for: item)
      return row
    }
    func rowOwner(of item: Item) -> Item? {
      var owner = item.machine == nil ? item : nil
      while let ancestor = owner.flatMap({ self.parent(of: $0) }), ancestor.machine == nil {
        owner = ancestor
      }
      return owner
    }
    private func configure(_ row: WorkspaceRow, for item: Item) {
      let parent = parent(of: item)
      var root: Item? = item.machine == nil ? item : nil
      while let ancestor = root.flatMap({ self.parent(of: $0) }), ancestor.machine == nil {
        root = ancestor
      }
      row.group = root
      row.treeParent = parent?.machine == nil ? parent : nil
      row.item = item
      row.loading = item.connectionAttempt
      let activity = status(for: item)
      let removal = item.workspace.flatMap { store.workspaceRemovals[$0.id] }
      row.status = item.workspace != nil ? activity : ""
      let tip =
        item.machine == nil
        ? "\(item.displayName) — \(activity == "unknown" ? "No reported activity" : activity)" : nil
      row.toolTip = removal?.action.label ?? tip
      row.groupTint = store.tint(for: row.group?.workspace?.id)
      row.tint = parent == nil || parent?.machine != nil ? nil : store.tint(for: item.tintID)
    }
    func outlineView(_ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any)
      -> NSView?
    {
      let item = item as! Item
      let w = item.workspace
      let parent = parent(of: item)
      let cell = WorkspaceCell(
        name: item.displayName, branch: item.isCoordinator ? nil : w?.sidebarBranch,
        tint: store.tint(for: item.tintID) ?? store.tint(for: rowOwner(of: item)?.workspace?.id),
        symbol: item.isCoordinator
          ? "person.2"
          : item.machine != nil
            ? "desktopcomputer"
            : (item.repositoryGroup || parent == nil || parent?.machine != nil
              ? "folder" : "arrow.triangle.branch"),
        diff: item.isCoordinator ? nil : w.flatMap { store.gitSummaries[$0.id] },
        loadingLabel: iconLoadingLabel(for: item),
        leadingInset: parent != nil && parent?.machine == nil
          ? SidebarGeometry.worktreeIconInset : 0,
        detail: item.machineLabel,
        image: item.isCoordinator ? CoordinatorMark.image : nil)
      if item.workspace == nil { cell.nameField.textColor = .secondaryLabelColor }
      let endpoint = w?.id.split(separator: "|").first.map(String.init) ?? ""
      if w?.id.contains("|") == true && store.remoteMessages[endpoint] != "Connected" {
        cell.alphaValue = 0.5
      }
      return cell
    }
    func status(for item: Item) -> String {
      if let endpoint = item.machine {
        return endpoint.isEmpty || store.remoteMessages[endpoint] == "Connected"
          ? "done" : "stopped"
      }
      if item.isCoordinator {
        return item.workspace?.coordinator.flatMap { store.panes[$0.pane] }?.activityStatus.rawValue
          ?? "unknown"
      }
      return item.workspace.map { store.activityStatus(for: $0).rawValue } ?? "unknown"
    }
    func outlineViewSelectionDidChange(_ notification: Notification) {
      guard !updating, let outline = notification.object as? NSOutlineView,
        let item = outline.item(atRow: outline.selectedRow) as? Item
      else { return }
      guard item.machine == nil else { return }
      if let w = item.workspace {
        store.select(w.id, surface: item.isCoordinator ? .coordinator : .terminals)
      } else {
        store.openCheckout(item.path)
      }
    }
    func dragSession(_ source: Item, grabY: CGFloat, outline: NSOutlineView) -> ReorderSession? {
      guard source.workspace != nil, !source.isCoordinator, store.workspaceCreations.isEmpty,
        store.workspaceRemovals.isEmpty
      else {
        return nil
      }
      let parent = parent(of: source)
      let siblings = (parent?.children ?? roots).filter { !$0.isCoordinator }
      guard let index = siblings.firstIndex(where: { $0 === source }) else { return nil }
      let centers = siblings.map { item -> CGFloat in
        var rect = outline.rect(ofRow: outline.row(forItem: item))
        if parent == nil || parent?.machine != nil, outline.isItemExpanded(item),
          let last = item.children.last
        {
          let lastRow = outline.row(forItem: last)
          if lastRow >= 0 { rect = rect.union(outline.rect(ofRow: lastRow)) }
        }
        return rect.midY
      }
      return ReorderSession(centers: centers, source: index, grabPosition: grabY)
    }
    func previewMove(_ source: Item, to: Int, outline: NSOutlineView) {
      let parent = parent(of: source)
      let siblings = (parent?.children ?? roots).filter { !$0.isCoordinator }
      guard siblings.indices.contains(to),
        let from = siblings.firstIndex(where: { $0 === source }), from != to
      else { return }
      updating = true
      NSAnimationContext.runAnimationGroup { context in
        context.duration = 0.22
        context.timingFunction = CAMediaTimingFunction(controlPoints: 0.22, 1, 0.36, 1)
        outline.beginUpdates()
        if let parent {
          let item = parent.children.remove(
            at: parent.children.firstIndex(where: { $0 === source })!)
          parent.children.insert(item, at: to + parent.children.filter(\.isCoordinator).count)
        } else {
          let item = roots.remove(at: from)
          roots.insert(item, at: to)
        }
        let offset = parent?.children.filter(\.isCoordinator).count ?? 0
        outline.moveItem(at: from + offset, inParent: parent, to: to + offset, inParent: parent)
        outline.endUpdates()
      }
      updating = false
    }
    func finishMove(commit: Bool, source: Item?, outline: NSOutlineView) {
      let ids = activeItems.filter { !$0.isCoordinator }.compactMap { $0.workspace?.id }
      guard commit, let sourceID = source?.workspace?.id ?? source?.children.first?.workspace?.id
      else {
        store.reorderingWorkspaces = false
        store.workspaceOrderGeneration &+= 1
        signature = "<cancelled-drag>"
        update(outline)
        return
      }
      let original = store.projectedWorkspaces
      let order = ids + original.map(\.id).filter { !ids.contains($0) }
      let endpoint = WorkspaceSelection.endpoint(of: sourceID)
      let ordered = order.filter { WorkspaceSelection.endpoint(of: $0) == endpoint }
      store.reorderingWorkspaces = false
      store.workspaceOrderGeneration &+= 1
      signature = "<moved>"
      let accepted = store.submitLayout(
        ["ReorderWorkspaces": ["workspaces": ordered]], endpoint: endpoint,
        preview: { layout in
          let current = layout.filter { WorkspaceSelection.endpoint(of: $0.id) == endpoint }
          guard Set(current.map(\.id)) == Set(ordered) else { return false }
          let byID = Dictionary(uniqueKeysWithValues: current.map { ($0.id, $0) })
          var reordered = ordered.compactMap { byID[$0] }.makeIterator()
          for index in layout.indices
          where
            WorkspaceSelection.endpoint(of: layout[index].id) == endpoint
          {
            if let workspace = reordered.next() { layout[index] = workspace }
          }
          return true
        })
      if !accepted { update(outline) }
    }
    func beginRename(_ item: Item, outline: NSOutlineView) {
      guard editing == nil, !item.isCoordinator, let workspace = item.workspace,
        !store.isWorkspacePending(workspace.id)
      else { return }
      let row = outline.row(forItem: item)
      guard row >= 0,
        let cell = outline.view(atColumn: 0, row: row, makeIfNecessary: true) as? WorkspaceCell
      else { return }
      editing = workspace.id
      store.editingWorkspace = workspace.id
      let nameRect = cell.nameField.convert(cell.nameField.bounds, to: outline)
      let editor = InlineNameEditor(
        frame: NSRect(
          x: nameRect.minX - 2, y: nameRect.midY - 11,
          width: max(
            40, outline.visibleRect.maxX - nameRect.minX - (outline.isExpandable(item) ? 32 : 6)),
          height: 22))
      editor.autoresizingMask = [.width]
      editor.theme = store.theme
      editor.setAccessibilityLabel("Workspace name")
      editor.stringValue = workspace.name
      // Preserve the stack's layout and branch label while hiding the old name.
      cell.titleRow.alphaValue = 0
      editor.completion = { [weak self, weak outline, weak editor, weak cell] value in
        guard let self, let outline else { return }
        if let value {
          self.store.rename(workspace.id, to: value)
          cell?.nameField.stringValue =
            self.store.projectedWorkspaces.first(where: { $0.id == workspace.id })?.name
            ?? workspace.name
        }
        cell?.titleRow.alphaValue = 1
        editor?.removeFromSuperview()
        self.editing = nil
        self.store.editingWorkspace = nil
        self.signature = "<rename-finished>"
        self.update(outline)
        self.store.focusTerminal()
      }
      // InlineNameEditor activates itself once it joins the window.
      outline.addSubview(editor)
    }
    func menu(_ item: Item?) -> NSMenu {
      let menu = NSMenu()
      menu.allowsContextMenuPlugIns = false
      menu.autoenablesItems = false
      guard let item else {
        menu.addItem(SidebarMenuItem(title: "New Workspace") { self.store.newWorkspace() })
        menu.addItem(SidebarMenuItem(title: "Open Workspace…") { self.store.openFolder() })
        return menu
      }
      if item.workspace == nil { return menu }
      guard let w = item.workspace else { return menu }
      guard !store.isWorkspacePending(w.id) else { return menu }
      func tintEntry(_ id: String) -> NSMenuItem {
        let entry = NSMenuItem(title: "Tint", action: nil, keyEquivalent: "")
        entry.submenu = store.tintMenu(for: id)
        if let color = store.tint(for: id) { entry.image = WorkspaceTint.swatch(color) }
        return entry
      }
      if item.isCoordinator {
        if let pane = w.coordinator?.pane {
          let resume = SidebarMenuItem(title: "Resume") { self.store.restartPane(pane) }
          resume.isEnabled =
            store.panes[pane].map {
              $0.needsResume && store.resumeState(for: $0)?.canResume == true
            } ?? false
          menu.addItem(resume)
          if let id = item.tintID { menu.addItem(tintEntry(id)) }
          menu.addItem(.separator())
          menu.addItem(SidebarMenuItem(title: "Close Coordinator") { self.store.close(pane) })
        }
        return menu
      }
      func add(_ name: String, _ action: @escaping () -> Void) {
        let entry = SidebarMenuItem(title: name, action: action)
        menu.addItem(entry)
      }
      add("Rename") { [weak self] in
        guard let self, let outline = self.outline else { return }
        DispatchQueue.main.async { self.beginRename(item, outline: outline) }
      }
      let coordinator = NSMenuItem(title: "Add Coordinator", action: nil, keyEquivalent: "")
      let choices = NSMenu()
      let endpoint = WorkspaceSelection.endpoint(of: w.id)
      for provider in ["claude", "codex"]
      where store.config?.enabledAgents.contains(provider) != false {
        let choice = SidebarMenuItem(title: AgentBrand.name(provider)) {
          self.store.addCoordinator(w.id, provider: provider)
        }
        choice.image = AgentBrand.image(provider)
        choice.isEnabled = store.availableAgents[endpoint]?.contains(provider) == true
        if !choice.isEnabled { choice.toolTip = "Not detected on this machine" }
        choices.addItem(choice)
      }
      coordinator.submenu = choices
      coordinator.isEnabled =
        store.capabilitiesByEndpoint[WorkspaceSelection.endpoint(of: w.id)]?.contains(
          "coordination_v1") == true && !w.hasCoordinator
        && !store.startingCoordinators.contains(w.id)
      menu.addItem(coordinator)
      menu.addItem(tintEntry(w.id))
      if w.activeRepository != nil {
        let reason = store.worktreeReason(w)
        let create = SidebarMenuItem(title: "New Worktree…") {
          self.store.select(w.id)
          self.store.execute("new_worktree")
        }
        create.isEnabled = reason == nil && !store.busy
        create.toolTip = reason
        menu.addItem(create)
        if let reason {
          let explanation = NSMenuItem(title: reason, action: nil, keyEquivalent: "")
          explanation.isEnabled = false
          explanation.indentationLevel = 1
          menu.addItem(explanation)
        }
        let open = SidebarMenuItem(title: "Open Worktree…") {
          self.store.select(w.id)
          self.store.openWorktree()
        }
        open.isEnabled =
          store.capabilitiesByEndpoint[WorkspaceSelection.endpoint(of: w.id)]?.contains(
            "workspace_containers") == true && !store.busy
        menu.addItem(open)
      }
      let root = parent(of: item) == nil || parent(of: item)?.machine != nil
      menu.addItem(.separator())
      add("Close Workspace") { self.store.closeWorkspace(w, group: root) }
      if !root && store.config?.allowWorkspaceDeletion != false {
        add("Delete Checkout…") { self.store.removeWorktree(w) }
      }
      return menu
    }
  }
}
final class WorkspaceOutline: NSOutlineView {
  weak var coordinator: WorkspaceSidebar.Coordinator?
  var dragOrigin: NSEvent?
  var draggedItem: WorkspaceSidebar.Item?
  var isReordering = false
  var reorderSession: ReorderSession?
  override func frameOfCell(atColumn column: Int, row: Int) -> NSRect {
    var frame = super.frameOfCell(atColumn: column, row: row)
    let inset = (item(atRow: row) as? WorkspaceSidebar.Item)?.topInset ?? 0
    frame.origin.y += inset
    frame.size.height = max(0, frame.height - inset)
    if (item(atRow: row) as? WorkspaceSidebar.Item)?.machine != nil {
      frame.origin.x = visibleRect.minX + SidebarGeometry.machineLeading
    } else {
      frame.origin.x += SidebarGeometry.workspaceContentInset
      // Machine headings group the list without adding workspace indentation.
      if coordinator?.roots.contains(where: { $0.machine != nil }) == true {
        frame.origin.x -= indentationPerLevel
      }
    }
    let trailing: CGFloat =
      item(atRow: row).map { isExpandable($0) } == true
        || (item(atRow: row) as? WorkspaceSidebar.Item)?.loading == true
        || (item(atRow: row) as? WorkspaceSidebar.Item)?.connectionAttempt == true ? 24 : 0
    let right = min(frame.maxX, visibleRect.maxX - 8 - trailing)
    frame.size.width = max(0, right - frame.minX)
    return frame
  }
  override func frameOfOutlineCell(atRow row: Int) -> NSRect {
    guard row >= 0, row < numberOfRows, let entry = item(atRow: row), isExpandable(entry),
      (entry as? WorkspaceSidebar.Item)?.connectionAttempt != true
    else {
      return .zero
    }
    var frame = super.frameOfOutlineCell(atRow: row)
    frame.origin.x = visibleRect.maxX - frame.width - 10
    let inset = (item(atRow: row) as? WorkspaceSidebar.Item)?.topInset ?? 0
    frame.origin.y = rect(ofRow: row).midY + inset / 2 - frame.height / 2
    return frame
  }
  func disclosureHitRect(atRow row: Int) -> NSRect {
    let icon = frameOfOutlineCell(atRow: row)
    guard !icon.isEmpty else { return .zero }
    return NSRect(x: icon.midX - 14, y: icon.midY - 14, width: 28, height: 28)
      .intersection(rect(ofRow: row)).intersection(visibleRect)
  }
  override func hitTest(_ point: NSPoint) -> NSView? {
    guard let hit = super.hitTest(point) else { return nil }
    let local = convert(point, from: superview)
    let row = row(at: local)
    if row >= 0, disclosureHitRect(atRow: row).contains(local) { return self }
    return hit
  }
  override func selectRowIndexes(_ indexes: IndexSet, byExtendingSelection extend: Bool) {
    // Programmatic selection (including AppKit collapse handling) bypasses the
    // delegate. Keep group headings out here as well as in mouseDown.
    let selectable = IndexSet(
      indexes.filter {
        guard $0 >= 0, $0 < numberOfRows,
          let item = item(atRow: $0) as? WorkspaceSidebar.Item
        else { return false }
        return item.workspace != nil
      })
    guard indexes.isEmpty || !selectable.isEmpty else { return }
    super.selectRowIndexes(selectable, byExtendingSelection: extend)
  }
  override func mouseDown(with event: NSEvent) {
    let point = convert(event.locationInWindow, from: nil)
    let row = self.row(at: point)
    guard row >= 0, let item = item(atRow: row) as? WorkspaceSidebar.Item else {
      // Empty-space clicks must not clear selection until the next store refresh.
      dragOrigin = nil
      draggedItem = nil
      reorderSession = nil
      return
    }
    guard coordinator?.editing == nil else {
      super.mouseDown(with: event)
      return
    }
    if isExpandable(item), disclosureHitRect(atRow: row).contains(point) {
      dragOrigin = nil
      draggedItem = nil
      reorderSession = nil
      NSAnimationContext.runAnimationGroup { context in
        context.duration = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 0 : 0.22
        context.timingFunction = CAMediaTimingFunction(controlPoints: 0.22, 1, 0.36, 1)
        if isItemExpanded(item) {
          animator().collapseItem(item)
        } else {
          animator().expandItem(item)
        }
      }
      return
    }
    if item.workspace == nil {
      dragOrigin = nil
      draggedItem = nil
      reorderSession = nil
      if let endpoint = item.machine { coordinator?.store.openMachineStatus(endpoint) }
      return
    }
    if !item.isCoordinator && event.clickCount == 2 {
      dragOrigin = nil
      draggedItem = nil
      coordinator?.beginRename(item, outline: self)
      return
    }
    reorderSession = nil
    dragOrigin = item.isCoordinator ? nil : event
    draggedItem = item.isCoordinator ? nil : item
    if selectedRow == row, let workspace = item.workspace {
      coordinator?.store.select(
        workspace.id, surface: item.isCoordinator ? .coordinator : .terminals)
    } else {
      selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
    }
  }
  override func mouseDragged(with event: NSEvent) {
    guard let coordinator, let origin = dragOrigin, let source = draggedItem else { return }
    if !isReordering {
      guard
        hypot(
          event.locationInWindow.x - origin.locationInWindow.x,
          event.locationInWindow.y - origin.locationInWindow.y) >= 4
      else { return }
      guard coordinator.store.canOrderWorkspaces else {
        coordinator.store.error =
          "Workspace reordering requires the updated daemon. Existing shells are still running."
        return
      }
      reorderSession = coordinator.dragSession(
        source, grabY: convert(origin.locationInWindow, from: nil).y, outline: self)
      guard reorderSession != nil else { return }
      coordinator.store.reorderingWorkspaces = true
      coordinator.store.workspaceOrderGeneration &+= 1
      isReordering = true
    }
    if let destination = reorderSession?.destination(
      at: convert(event.locationInWindow, from: nil).y)
    {
      coordinator.previewMove(source, to: destination, outline: self)
    }
  }
  override func mouseUp(with event: NSEvent) {
    if isReordering {
      let point = convert(event.locationInWindow, from: nil)
      coordinator?.finishMove(
        commit: visibleRect.contains(point), source: draggedItem, outline: self)
    }
    isReordering = false
    reorderSession = nil
    dragOrigin = nil
    draggedItem = nil
  }
  override func rightMouseDown(with event: NSEvent) {
    let row = row(at: convert(event.locationInWindow, from: nil))
    let item = row >= 0 ? item(atRow: row) as? WorkspaceSidebar.Item : nil
    guard let menu = coordinator?.menu(item) else { return }
    NSMenu.popUpContextMenu(menu, with: event, for: self)
  }
}
final class WorkspaceRow: NSTableRowView {
  var tint: NSColor?
  var groupTint: NSColor?
  weak var group: WorkspaceSidebar.Item?
  weak var treeParent: WorkspaceSidebar.Item?
  weak var item: WorkspaceSidebar.Item?
  private let activityIndicator = ActivityDotView()
  var status = "" {
    didSet {
      activityIndicator.isHidden = status.isEmpty
      activityIndicator.status = ActivityStatus(status)
    }
  }
  private let loadingSpinner = NSProgressIndicator()
  var loading = false {
    didSet {
      guard loading != oldValue else { return }
      if loading { loadingSpinner.startAnimation(nil) } else { loadingSpinner.stopAnimation(nil) }
      loadingSpinner.isHidden = !loading
    }
  }
  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    loadingSpinner.style = .spinning
    loadingSpinner.controlSize = .small
    loadingSpinner.isIndeterminate = true
    loadingSpinner.isDisplayedWhenStopped = false
    loadingSpinner.isHidden = true
    loadingSpinner.setAccessibilityLabel("Loading remote workspaces")
    addSubview(loadingSpinner)
    activityIndicator.isHidden = true
    addSubview(activityIndicator)
  }
  required init?(coder: NSCoder) { fatalError("unsupported") }
  override func layout() {
    super.layout()
    let viewport = enclosingScrollView?.contentView
    let visible = viewport.map { convert($0.bounds, from: $0) } ?? bounds
    activityIndicator.frame = NSRect(
      x: visible.minX + SidebarGeometry.statusLeading + SidebarGeometry.statusSize / 2 - 5,
      y: bounds.midY - 5, width: 10, height: 10)
    loadingSpinner.frame = NSRect(
      x: visible.maxX - 24, y: bounds.midY + (item?.topInset ?? 0) / 2 - 7,
      width: 14, height: 14)
  }
  override var interiorBackgroundStyle: NSView.BackgroundStyle { .normal }
  private func fill(_ color: NSColor, opacity: CGFloat) {
    color.withAlphaComponent(opacity).setFill()
    let viewport = enclosingScrollView?.contentView
    let visible = viewport.map { convert($0.bounds, from: $0) } ?? bounds
    var rect = bounds.intersection(visible).insetBy(dx: 4, dy: 0)
    var leading: CGFloat = item?.machine != nil ? 4 : SidebarGeometry.workspaceHighlightLeading
    if let item, let group, item !== group,
      let outline = enclosingScrollView?.documentView as? WorkspaceOutline
    {
      let row = outline.row(forItem: item)
      if row >= 0 {
        // Pad the icon while keeping the connector outside the highlight.
        leading =
          convert(outline.frameOfCell(atColumn: 0, row: row), from: outline).minX
          + SidebarGeometry.worktreeIconInset - SidebarGeometry.selectionPadding
      }
    }
    let right = rect.maxX
    rect.origin.x = max(rect.minX, leading)
    rect.size.width = max(0, right - rect.minX)
    NSBezierPath(roundedRect: rect, xRadius: 7, yRadius: 7).fill()
  }
  override func drawBackground(in dirtyRect: NSRect) {
    super.drawBackground(in: dirtyRect)
    if let groupTint, let group,
      let outline = enclosingScrollView?.documentView as? WorkspaceOutline
    {
      let first = outline.row(forItem: group)
      func lastVisible(_ node: WorkspaceSidebar.Item) -> Int {
        max(outline.row(forItem: node), node.children.map(lastVisible).max() ?? -1)
      }
      let last = lastVisible(group)
      if first >= 0 {
        let rect = outline.rect(ofRow: first).union(outline.rect(ofRow: max(first, last)))
        var local = convert(rect, from: outline)
        local.origin.x = SidebarGeometry.workspaceHighlightLeading
        local.size.width = bounds.width - SidebarGeometry.workspaceHighlightLeading - 4
        NSGraphicsContext.saveGraphicsState()
        bounds.clip()
        groupTint.withAlphaComponent(0.035).setFill()
        NSBezierPath(roundedRect: local.insetBy(dx: 0, dy: 1), xRadius: 7, yRadius: 7).fill()
        NSGraphicsContext.restoreGraphicsState()
      }
    }
    if !isSelected, let tint { fill(tint, opacity: 0.035) }
    if !isSelected { drawTree() }
  }
  private func drawTree() {
    if let outline = enclosingScrollView?.documentView as? WorkspaceOutline, let item {
      let tree = NSBezierPath()
      let myRow = outline.row(forItem: item)
      let cell = convert(outline.frameOfCell(atColumn: 0, row: myRow), from: outline)
      var child = item
      var ancestor = treeParent
      var direct = true
      while let parent = ancestor, parent.machine == nil {
        let row = outline.row(forItem: parent)
        let parentCell = convert(outline.frameOfCell(atColumn: 0, row: row), from: outline)
        let x = parentCell.minX + 6
        let last = parent.children.last === child
        if direct || !last {
          tree.move(to: NSPoint(x: x, y: bounds.minY - 2))
          tree.line(to: NSPoint(x: x, y: direct && last ? bounds.midY : bounds.maxY + 2))
        }
        if direct {
          tree.move(to: NSPoint(x: x, y: bounds.midY))
          tree.line(to: NSPoint(x: cell.minX - 4, y: bounds.midY))
        }
        direct = false
        child = parent
        ancestor = outline.coordinator?.parent(of: parent)
      }
      if item.machine == nil, outline.isItemExpanded(item), !item.children.isEmpty {
        let x = cell.minX + 6
        tree.move(to: NSPoint(x: x, y: bounds.midY + 11))
        tree.line(to: NSPoint(x: x, y: bounds.maxY + 2))
      }
      NSColor.secondaryLabelColor.withAlphaComponent(0.25).setStroke()
      tree.lineWidth = 1
      tree.stroke()
    }
  }
  override func drawSelection(in dirtyRect: NSRect) {
    let outline = enclosingScrollView?.documentView as? WorkspaceOutline
    fill(
      tint ?? groupTint ?? outline?.coordinator?.store.theme.accent ?? .controlAccentColor,
      opacity: 0.10)
    drawTree()
  }
}
final class SidebarMenuItem: NSMenuItem {
  let callback: () -> Void
  init(title: String, action: @escaping () -> Void) {
    callback = action
    super.init(title: title, action: #selector(invoke), keyEquivalent: "")
    target = self
  }
  required init(coder: NSCoder) { fatalError("unsupported") }
  @objc func invoke() { callback() }
}

/// Hiding a scroller does not stop trackpad gestures from changing the clip's x offset.
final class WorkspaceClipView: NSClipView {
  override func constrainBoundsRect(_ proposedBounds: NSRect) -> NSRect {
    var result = super.constrainBoundsRect(proposedBounds)
    result.origin.x = 0
    return result
  }
}

final class WorkspaceScrollView: NSScrollView {
  private var scrollerTimer: Timer?

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    scrollerStyle = .overlay
    hasVerticalScroller = true
    hasHorizontalScroller = false
    autohidesScrollers = true
    verticalScroller?.alphaValue = 0
  }
  required init?(coder: NSCoder) { fatalError("unsupported") }
  deinit { scrollerTimer?.invalidate() }

  override func scrollWheel(with event: NSEvent) {
    if event.scrollingDeltaY != 0,
      let documentView, documentView.frame.height > contentView.bounds.height
    {
      verticalScroller?.alphaValue = 1
      scrollerTimer?.invalidate()
      scrollerTimer = Timer.scheduledTimer(withTimeInterval: 0.75, repeats: false) {
        [weak self] _ in
        NSAnimationContext.runAnimationGroup { context in
          context.duration = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 0 : 0.2
          self?.verticalScroller?.animator().alphaValue = 0
        }
      }
    }
    super.scrollWheel(with: event)
  }

  override func rightMouseDown(with event: NSEvent) {
    guard let outline = documentView as? WorkspaceOutline,
      let menu = outline.coordinator?.menu(nil)
    else { return }
    NSMenu.popUpContextMenu(menu, with: event, for: self)
  }
  override func layout() {
    super.layout()
    guard let outline = documentView as? NSOutlineView else { return }
    let width = contentView.bounds.width
    if outline.frame.width != width {
      outline.setFrameSize(NSSize(width: width, height: outline.frame.height))
    }
    if let column = outline.tableColumns.first, column.width != width { column.width = width }
    if contentView.bounds.origin.x != 0 {
      contentView.scroll(to: NSPoint(x: 0, y: contentView.bounds.origin.y))
    }
  }
}
