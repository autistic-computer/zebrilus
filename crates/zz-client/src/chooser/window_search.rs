use zz_protocol::{ChooseTreeState, ChooseTreeTarget};

#[derive(Default)]
pub struct WindowChooserSearch {
    pub expanded: Vec<ChooseTreeTarget>,
    pub pending: Option<(ChooseTreeTarget, bool)>,
    pub selection: Option<ChooseTreeTarget>,
}

impl WindowChooserSearch {
    pub fn advance(&mut self, state: &ChooseTreeState, searching: bool) -> Option<(u32, bool)> {
        if let Some((target, expanded)) = self.pending {
            if state.items.iter().any(|item| {
                item.target == target
                    && item.expanded() != expanded
                    && (!expanded || item.has_children())
            }) {
                return None;
            }
            self.pending = None;
        }
        let next = if searching {
            state.items.iter().enumerate().find_map(|(index, item)| {
                (item.has_children() && !item.expanded()).then_some((index, item.target, true))
            })
        } else {
            self.expanded.iter().rev().find_map(|target| {
                state.items.iter().enumerate().find_map(|(index, item)| {
                    (item.target == *target && item.expanded()).then_some((index, *target, false))
                })
            })
        };
        let Some((index, target, expanded)) = next else {
            if !searching {
                self.expanded.clear();
            }
            return None;
        };
        if expanded && !self.expanded.contains(&target) {
            self.expanded.push(target);
        }
        self.pending = Some((target, expanded));
        Some((index as u32, expanded))
    }
}
