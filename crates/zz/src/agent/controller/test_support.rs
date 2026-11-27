use std::{cell::RefCell, rc::Rc};

use ala::{AppContext as _, TestAppContext};

use super::*;

pub(super) fn thread() -> AgentThread {
    AgentThread::new(AgentProvider::Codex, PathBuf::from("/workspace"), None)
}

pub(super) const PNG_PIXEL: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

pub(super) fn attachment() -> Arc<Image> {
    Arc::new(Image::from_bytes(ImageFormat::Png, PNG_PIXEL.to_vec()))
}

pub(super) fn proxy_controller(
    cx: &mut TestAppContext,
) -> (
    Entity<AgentController>,
    Rc<RefCell<Vec<(PaneId, AgentRequest)>>>,
) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let sink = mux.update(cx, |mux, _| {
            mux.set_agent_client_instance_id_for_test(zz_protocol::ClientInstanceId(1));
            mux.record_agent_requests_for_test()
        });
        let controller = cx.new(|_| AgentController::new());
        controller.update(cx, |controller, _| controller.attach_mux(mux));
        (controller, sink)
    })
}

pub(super) fn ready_pane(controller: &mut AgentController, pane: PaneId) {
    let mut thread = thread();
    thread.connection = AgentConnectionState::Ready;
    thread.session_capabilities.images = true;
    controller.panes.insert(pane, thread);
    controller.viewports.insert(pane, PaneViewport::default());
    controller.retained_panes.insert(pane);
}

pub(super) fn item(seq: u64, payload: AgentStreamPayload) -> AgentStreamItem {
    AgentStreamItem { seq, payload }
}

pub(super) fn json(value: &impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).expect("the ACP schema encodes")
}

pub(super) fn turn_finished(reason: StopReason) -> AgentStreamPayload {
    AgentStreamPayload::PromptFinished {
        turn_id: 1,
        outcome: AgentPromptOutcome::Finished {
            stop_reason: json(&reason),
        },
    }
}

pub(super) fn reply(id: &str, answers: &[&str]) -> AgentQuestionAnswer {
    AgentQuestionAnswer {
        id: id.to_owned(),
        answers: answers.iter().map(|answer| (*answer).to_owned()).collect(),
    }
}
