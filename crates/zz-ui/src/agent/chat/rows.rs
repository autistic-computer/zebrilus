use super::*;

impl Row {
    pub(super) fn key(&self, view: &ChatView) -> RowKey {
        match self {
            Self::Turn(index) => view
                .turns
                .get(*index)
                .map_or(RowKey::Empty, |turn| RowKey::Turn(turn.id)),
            Self::Background(unit) => match unit {
                Unit::User(id)
                | Unit::Text(id)
                | Unit::Subagent(id)
                | Unit::Plan(id)
                | Unit::Notice(id)
                | Unit::Page(id) => RowKey::Item(id.clone()),
                Unit::Run(run) => run
                    .items
                    .first()
                    .map_or(RowKey::Empty, |id| RowKey::Item(id.clone())),
                Unit::Agents(ids) => ids
                    .first()
                    .map_or(RowKey::Empty, |id| RowKey::Item(id.clone())),
                Unit::Ask(request) | Unit::Record(request) => RowKey::Request(*request),
                Unit::Child(card) => RowKey::Child(card.anchor.clone()),
                Unit::Sent(sent) => sent
                    .run
                    .items
                    .first()
                    .map_or(RowKey::Empty, |id| RowKey::Item(id.clone())),
                Unit::Peer(note) => RowKey::Item(note.item.clone()),
            },
        }
    }
}

pub(super) fn holds(unit: &Unit, id: &ItemId) -> bool {
    match unit {
        Unit::User(held)
        | Unit::Text(held)
        | Unit::Subagent(held)
        | Unit::Plan(held)
        | Unit::Notice(held)
        | Unit::Page(held) => held == id,
        Unit::Run(run) => run.items.contains(id),
        Unit::Sent(sent) => sent.run.items.contains(id),
        Unit::Peer(note) => note.item == *id,
        Unit::Agents(ids) => ids.contains(id),
        Unit::Ask(_) | Unit::Record(_) | Unit::Child(_) => false,
    }
}

pub(super) fn thread_rows(view: &ChatView, fold: &ChildFold) -> Vec<Row> {
    let mut pending = chat_rows::background_rows(view, fold)
        .into_iter()
        .peekable();
    let mut rows = Vec::new();
    for (index, turn) in view.turns.iter().enumerate() {
        while let Some(unit) = pending
            .next_if(|unit| chat_rows::unit_at(view, unit).is_some_and(|at| at < turn.started_at))
        {
            rows.push(Row::Background(unit));
        }
        rows.push(Row::Turn(index));
    }
    rows.extend(pending.map(Row::Background));
    rows
}
