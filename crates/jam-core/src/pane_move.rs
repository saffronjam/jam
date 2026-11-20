use crate::*;
use anyhow::{Context, Result, ensure};

fn locate(state: &State, pane: &str) -> Option<(usize, usize)> {
    for (w, workspace) in state.workspaces.iter().enumerate() {
        for (t, tab) in workspace.tabs.iter().enumerate() {
            let mut ids = Vec::new();
            tab.root.ids(&mut ids);
            if ids.iter().any(|id| id == pane) {
                return Some((w, t));
            }
        }
    }
    None
}
fn adjacent(node: &mut Node, source: &str, target: &str, placement: PanePlacement) -> bool {
    match node {
        Node::Leaf(id) if id == target => {
            let before = matches!(placement, PanePlacement::Left | PanePlacement::Above);
            *node = Node::Split {
                vertical: matches!(placement, PanePlacement::Above | PanePlacement::Below),
                ratio: 0.5,
                a: Box::new(Node::Leaf(if before { source } else { target }.into())),
                b: Box::new(Node::Leaf(if before { target } else { source }.into())),
            };
            true
        }
        Node::Split { a, b, .. } => {
            adjacent(a, source, target, placement) || adjacent(b, source, target, placement)
        }
        _ => false,
    }
}
fn swap(node: &mut Node, a: &str, b: &str) {
    match node {
        Node::Leaf(id) if id == a => *id = b.into(),
        Node::Leaf(id) if id == b => *id = a.into(),
        Node::Split {
            a: left, b: right, ..
        } => {
            swap(left, a, b);
            swap(right, a, b);
        }
        _ => {}
    }
}

/// Build a complete candidate before touching live metadata or layout.
pub(crate) fn moved(state: &State, pane: &str, destination: PaneDestination) -> Result<State> {
    let (source_w, source_t) = locate(state, pane).context("source pane missing")?;
    let name = state.workspaces[source_w].tabs[source_t].name.clone();
    let empty_tab = if let PaneDestination::Tab { ref tab } = destination {
        state
            .workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .find(|t| t.id == *tab && matches!(t.root, Node::Empty))
            .map(|t| t.id.clone())
    } else {
        None
    };
    let (target, placement, workspace) = match destination {
        PaneDestination::Pane {
            pane: target,
            placement,
        } => {
            let (w, _) = locate(state, &target).context("target pane missing")?;
            (Some(target), placement, state.workspaces[w].id.clone())
        }
        PaneDestination::Tab { tab } => {
            let (w, t) = state
                .workspaces
                .iter()
                .enumerate()
                .find_map(|(w, workspace)| {
                    workspace.tabs.iter().find(|t| t.id == tab).map(|t| (w, t))
                })
                .context("target tab missing")?;
            let mut ids = Vec::new();
            t.root.ids(&mut ids);
            let target = ids.into_iter().find(|id| id != pane);
            if target.is_none() && empty_tab.is_none() {
                return Ok(state.clone());
            }
            (target, PanePlacement::Right, state.workspaces[w].id.clone())
        }
        PaneDestination::Workspace { workspace } => {
            ensure!(
                state.workspaces.iter().any(|w| w.id == workspace),
                "target workspace missing"
            );
            (None, PanePlacement::Right, workspace)
        }
    };
    if target.as_deref() == Some(pane) {
        return Ok(state.clone());
    }
    let mut candidate = state.clone();
    if matches!(placement, PanePlacement::Swap) {
        let target = target.context("swap requires a target pane")?;
        for w in &mut candidate.workspaces {
            for t in &mut w.tabs {
                swap(&mut t.root, pane, &target);
            }
        }
    } else {
        // Keep destination containers until insertion has completed.
        let mut settings = crate::config::host_settings();
        settings.close_empty_workspaces = false;
        crate::server::remove_pane_from_layout(&mut candidate, pane, &settings);
        if let Some(target) = target {
            let (w, t) = locate(&candidate, &target).context("target disappeared")?;
            ensure!(
                adjacent(
                    &mut candidate.workspaces[w].tabs[t].root,
                    pane,
                    &target,
                    placement
                ),
                "could not place pane"
            );
        } else if let Some(tab) = empty_tab {
            candidate
                .workspaces
                .iter_mut()
                .flat_map(|w| &mut w.tabs)
                .find(|t| t.id == tab)
                .unwrap()
                .root = Node::Leaf(pane.into());
        } else {
            candidate
                .workspaces
                .iter_mut()
                .find(|w| w.id == workspace)
                .unwrap()
                .tabs
                .push(Tab {
                    agent_role: None,
                    id: id(),
                    name,
                    root: Node::Leaf(pane.into()),
                });
        }
    }
    for w in &mut candidate.workspaces {
        let mut ids = Vec::new();
        for t in &w.tabs {
            t.root.ids(&mut ids);
        }
        for p in &mut candidate.panes {
            if ids.contains(&p.id) {
                p.workspace = w.id.clone();
            }
        }
        if w.active_pane.as_ref().is_some_and(|id| !ids.contains(id)) {
            w.active_pane = None;
        }
        if ids.iter().any(|id| id == pane) {
            w.active_pane = Some(pane.into());
        }
    }
    Ok(candidate)
}
