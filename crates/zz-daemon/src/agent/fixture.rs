use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use agent_client_protocol::{
    Agent, Client as AcpClientRole, ConnectTo, ConnectionTo,
    schema::v1::{
        AgentCapabilities, CancelRequestNotification, ContentBlock, ContentChunk,
        InitializeRequest, InitializeResponse, LoadSessionRequest, LoadSessionResponse,
        NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionId,
        PermissionOptionKind, PromptRequest, PromptResponse, RequestPermissionRequest,
        SessionNotification, SessionUpdate, StopReason, TextContent, ToolCallStatus,
        ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    },
};
use zz_protocol::{AgentAutoApprove, AgentProvider};

use crate::agent::{
    host::{PaneRunner, RuntimeChannels},
    runtime::run_agent_connection,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Behavior {
    Chunk,
    Echo,
    AskPermission,
    AskKindedPermission(ToolKind),
    Hang,
    FailFirst,
}

impl Behavior {
    const fn permission_kind(self) -> Option<ToolKind> {
        match self {
            Self::AskKindedPermission(kind) => Some(kind),
            _ => None,
        }
    }
}

pub(crate) fn fixture_runner(
    provider: AgentProvider,
    behavior: Behavior,
    auto_approve: AgentAutoApprove,
    load: bool,
) -> PaneRunner {
    Box::new(move |channels: RuntimeChannels| {
        *channels.auto_approve.lock() = auto_approve;
        Box::pin(run_agent_connection(
            provider,
            channels.auto_approve,
            fixture_agent(behavior, load),
            channels.permission_ids,
            channels.journal,
            channels.commands,
            channels.controls,
            channels.events,
        ))
    })
}

pub(crate) fn fixture_agent(behavior: Behavior, load: bool) -> impl ConnectTo<AcpClientRole> {
    let prompts = Arc::new(AtomicUsize::new(0));
    let sessions = AtomicUsize::new(0);
    Agent
        .builder()
        .on_receive_notification(
            async move |_: CancelRequestNotification, _| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |initialize: InitializeRequest, responder, _| {
                responder.respond(
                    InitializeResponse::new(initialize.protocol_version)
                        .agent_capabilities(AgentCapabilities::new().load_session(load)),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: LoadSessionRequest,
                        responder,
                        connection: ConnectionTo<AcpClientRole>| {
                if behavior == Behavior::Echo {
                    connection.send_notification(SessionNotification::new(
                        request.session_id,
                        SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::Text(
                            TextContent::new("loaded user history"),
                        ))),
                    ))?;
                    return responder.respond(LoadSessionResponse::new());
                }
                responder.respond_with_error(
                    agent_client_protocol::Error::invalid_params()
                        .data("fixture session cannot be loaded"),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_: NewSessionRequest, responder, _| {
                let session = sessions.fetch_add(1, Ordering::Relaxed);
                let id = if session == 0 {
                    "fixture-session".to_owned()
                } else {
                    format!("fixture-session-{session}")
                };
                responder.respond(NewSessionResponse::new(id))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |prompt: PromptRequest,
                        responder,
                        connection: ConnectionTo<AcpClientRole>| {
                let turn = prompts.fetch_add(1, Ordering::Relaxed);
                let session_id = prompt.session_id.clone();
                if behavior == Behavior::Echo {
                    for content in prompt.prompt {
                        connection.send_notification(SessionNotification::new(
                            session_id.clone(),
                            SessionUpdate::UserMessageChunk(ContentChunk::new(content)),
                        ))?;
                    }
                }
                connection.send_notification(SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new(format!("turn {turn}")),
                    ))),
                ))?;
                match behavior {
                    Behavior::FailFirst if turn == 0 => responder.respond_with_error(
                        agent_client_protocol::Error::internal_error().data("rate limited"),
                    ),
                    Behavior::Chunk | Behavior::Echo | Behavior::FailFirst => {
                        responder.respond(PromptResponse::new(StopReason::EndTurn))
                    }
                    Behavior::Hang => Ok(()),
                    Behavior::AskPermission | Behavior::AskKindedPermission(_) => {
                        let tool = ToolCallUpdate::new(
                            "tool-1",
                            ToolCallUpdateFields::new()
                                .status(ToolCallStatus::Pending)
                                .title("run it".to_owned())
                                .kind(behavior.permission_kind()),
                        );
                        let permission = connection.send_request(RequestPermissionRequest::new(
                            session_id,
                            tool,
                            vec![
                                PermissionOption::new(
                                    PermissionOptionId::new("allow"),
                                    "Allow",
                                    PermissionOptionKind::AllowOnce,
                                ),
                                PermissionOption::new(
                                    PermissionOptionId::new("deny"),
                                    "Deny",
                                    PermissionOptionKind::RejectOnce,
                                ),
                            ],
                        ));
                        connection.spawn(async move {
                            permission.block_task().await?;
                            responder.respond(PromptResponse::new(StopReason::EndTurn))?;
                            Ok(())
                        })
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
}
