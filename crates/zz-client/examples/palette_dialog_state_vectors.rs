use std::error::Error;

use serde_json::{Value, json};
use zz_client::{ClientCore, CoreEvent};
use zz_protocol::{
    ChooseTreeItem, ChooseTreeKind, ChooseTreeSearchState, ChooseTreeState, ChooseTreeTarget,
    CommandPromptKind, CommandPromptMode, CommandPromptState, CommandPromptType, Event,
    EventPayload, PromptCursor, ProtocolMessage, SessionId, encode_protocol_message,
};

fn open(count: usize, selected: u32) -> EventPayload {
    EventPayload::ChooseTree {
        state: Some(ChooseTreeState {
            items: (0..count)
                .map(|index| ChooseTreeItem {
                    label: format!("label {index}"),
                    detail: String::new(),
                    target: ChooseTreeTarget::Session(SessionId(index as u64)),
                    depth: 0,
                    flags: 0,
                    pane_kind: None,
                    key: String::new(),
                    text: String::new(),
                })
                .collect(),
            search: Some(ChooseTreeSearchState {
                query: "open".to_owned(),
                reverse: false,
            }),
            selected,
            kind: ChooseTreeKind::Windows,
            filter_no_matches: false,
            prompt: "prompt".to_owned(),
            help: true,
        }),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut core = ClientCore::new();
    let mut cases: Vec<Value> = Vec::new();
    let mut prompt_revision = 0_u64;
    let mut chooser_revision = 0_u64;
    let mut push = |payload: EventPayload| -> Result<(), Box<dyn Error>> {
        let message = ProtocolMessage::Event(Event {
            sequence: 0,
            payload,
        });
        let bytes = encode_protocol_message(&message)?;
        core.handle_message(message);
        while let Some(event) = core.poll_event() {
            match event {
                CoreEvent::CommandPromptChanged => prompt_revision += 1,
                CoreEvent::ChooseTreeChanged => chooser_revision += 1,
                _ => {}
            }
        }
        let prompt = core.command_prompt();
        let chooser = core.choose_tree();
        cases.push(json!({"bytes":bytes,"prompt":prompt.map(|state| &state.input),
            "chooser":chooser.map(|state| json!({"rows":state.items.len(),"selected":state.selected,
                "search":state.search.as_ref().map(|state| json!({"query":state.query,"reverse":state.reverse}))})),
            "prompt_revision":prompt_revision,"chooser_revision":chooser_revision}));
        Ok(())
    };
    push(EventPayload::ChooseTreeUpdate {
        search: None,
        selected: u32::MAX,
    })?;
    for count in [0, 1, 2, 15, 128, 4096] {
        for selected in [0, 1, 15, u32::MAX] {
            push(open(count, selected))?;
            for updated in [0, 1, 15, u32::MAX] {
                push(EventPayload::ChooseTreeUpdate {
                    search: Some(ChooseTreeSearchState {
                        query: format!("search {updated} λ"),
                        reverse: updated == 0,
                    }),
                    selected: updated,
                })?;
            }
            push(EventPayload::ChooseTreeUpdate {
                search: None,
                selected,
            })?;
            push(EventPayload::ChooseTree { state: None })?;
            push(EventPayload::ChooseTreeUpdate {
                search: None,
                selected,
            })?;
        }
    }
    for input in ["", "value λ\0🦀"] {
        push(EventPayload::CommandPrompt {
            state: Some(CommandPromptState {
                prompt: "prompt".to_owned(),
                input: input.to_owned(),
                cursor: u32::MAX,
                kind: CommandPromptKind::Value,
                history: vec!["history".to_owned()],
                prompt_type: CommandPromptType::Search,
                mode: CommandPromptMode::Incremental,
                no_freeze: true,
                pane: None,
                command_mode: false,
                prompt_cursor: PromptCursor::default(),
            }),
        })?;
    }
    push(EventPayload::CommandPrompt { state: None })?;
    push(EventPayload::CommandPrompt { state: None })?;
    println!("{}", json!({"cases":cases}));
    Ok(())
}
