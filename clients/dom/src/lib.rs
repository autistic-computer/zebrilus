mod agent;
mod appearance;
mod events;
mod overlays;
mod palette;
mod terminal;
mod tree;

use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use zz_client::{ClientCore, Outbound};
use zz_protocol::{
    AttachOperation, ClientHello, ClientInstanceId, ClientKind, CommandInvocation, CommandRequest,
    Hello, InputMessage, PROTOCOL_VERSION, PaneId, ProtocolMessage, decode_protocol_frame,
    encode_protocol_message,
};
use zz_terminal::TerminalColorScheme;

#[wasm_bindgen]
pub struct Client {
    core: ClientCore,
    outbox: Vec<Vec<u8>>,
    next_request: u64,
    instance: u64,
    history: terminal::History,
    images: terminal::Images,
    views: terminal::Views,
    agents: agent::Agents,
    sidebar: tree::Sidebar,
    overlays: overlays::Overlays,
    palette: palette::Palette,
}

#[wasm_bindgen]
impl Client {
    #[wasm_bindgen(constructor)]
    pub fn new(instance: f64) -> Self {
        let mut core = ClientCore::new();
        core.retain_history();
        Self {
            core,
            outbox: Vec::new(),
            next_request: 1,
            instance: (instance as u64).max(1),
            history: terminal::History::default(),
            images: terminal::Images::default(),
            views: terminal::Views::default(),
            agents: agent::Agents::default(),
            sidebar: tree::Sidebar::default(),
            overlays: overlays::Overlays::default(),
            palette: palette::Palette::default(),
        }
    }

    pub fn hello(&mut self, session: &str, dark: bool) {
        let mut hello = Hello::from_client(ClientHello {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: ClientInstanceId(self.instance),
            kind: ClientKind::Interactive,
            device_name: Some("Browser (HTML)".into()),
            capabilities: vec![
                ClientHello::CLIENT_TERMINAL_CAPABILITY.into(),
                ClientHello::CLIENT_UTF8_CAPABILITY.into(),
            ],
            color_scheme: Some(scheme(dark)),
            origin: None,
            working_directory: None,
            environment: Vec::new(),
            process_id: 0,
        });
        hello.attach = Some(AttachOperation::Session(session.to_owned()));
        self.send(&ProtocolMessage::Hello(hello));
    }

    pub fn receive(&mut self, frame: &[u8]) -> Result<String, JsError> {
        let message =
            decode_protocol_frame(frame).map_err(|error| JsError::new(&error.to_string()))?;
        self.core.handle_message(message);
        while let Some(outbound) = self.core.poll_outbound() {
            let message = match outbound {
                Outbound::RequestFull(pane) => ProtocolMessage::RequestFull { pane },
                Outbound::TreeSync => ProtocolMessage::TreeSync,
            };
            self.send(&message);
        }
        let mut events = Vec::new();
        while let Some(event) = self.core.poll_event() {
            events.push(events::describe(self, event));
        }
        Ok(Value::Array(events).to_string())
    }

    pub fn outbound(&mut self) -> Option<Vec<u8>> {
        if self.outbox.is_empty() {
            None
        } else {
            Some(self.outbox.remove(0))
        }
    }

    pub fn layout_generation(&self) -> f64 {
        self.core.layout_generation() as f64
    }

    pub fn command(&mut self, name: &str, args: Vec<String>) -> f64 {
        self.invoke(CommandInvocation::new(name, args)) as f64
    }

    pub fn attach(&mut self, session: &str) {
        self.send(&ProtocolMessage::Attach {
            session: session.to_owned(),
        });
    }

    pub fn color_scheme(&mut self, dark: bool) {
        self.send(&ProtocolMessage::SetColorScheme(scheme(dark)));
    }

    pub fn focus(&mut self, focused: bool) {
        self.input(InputMessage::ClientFocus { focused });
    }

    pub fn float_pane(&mut self, pane: f64) {
        for command in zz_client::floating::float_pane_commands(pane_id(pane), None) {
            self.invoke(command);
        }
    }

    pub fn status(&self) -> String {
        json!({
            "prefix": self.core.prefix_armed(),
            "keyTable": self.core.key_table().map(|(table, _)| table.to_owned()),
            "readOnly": self.core.attached_read_only(),
        })
        .to_string()
    }
}

impl Client {
    fn invoke(&mut self, command: CommandInvocation) -> u64 {
        let request_id = self.next_request;
        self.next_request += 1;
        self.send(&ProtocolMessage::CommandRequest(CommandRequest {
            request_id,
            command,
            prepared: false,
        }));
        request_id
    }

    fn input(&mut self, input: InputMessage) {
        self.send(&ProtocolMessage::Input(input));
    }

    fn send(&mut self, message: &ProtocolMessage) {
        if let Ok(frame) = encode_protocol_message(message) {
            self.outbox.push(frame);
        }
    }
}

fn pane_id(pane: f64) -> PaneId {
    PaneId(pane as u64)
}

fn scheme(dark: bool) -> TerminalColorScheme {
    if dark {
        TerminalColorScheme::Dark
    } else {
        TerminalColorScheme::Light
    }
}
