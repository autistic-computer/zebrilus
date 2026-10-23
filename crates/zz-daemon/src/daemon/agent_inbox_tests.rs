use super::super::agent_publisher::Publisher;
use super::super::tests::take_reliable_messages;
use super::*;

struct Fixture {
    shared: Arc<Shared>,
    inbox: AgentInbox,
    pane: PaneId,
    mailbox: Arc<OutboundMailbox>,
}

fn fixture() -> Fixture {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "agent-inbox"]),
        )
        .expect("mux setup");
    shared
        .attach(client, context.session.expect("session"))
        .expect("attach");
    take_reliable_messages(&mailbox);
    let inbox = AgentInbox::new(&shared);
    Fixture {
        shared,
        inbox,
        pane: context.pane.expect("terminal pane"),
        mailbox,
    }
}

fn spec() -> AgentPaneSpec {
    AgentPaneSpec {
        provider: zz_protocol::AgentProvider::Codex,
        cwd: PathBuf::from("/"),
        resume_session: None,
        auto_approve: None,
        workspace: AgentWorkspaceEnvironment::default(),
        lens: None,
    }
}

fn runtime(shared: &Arc<Shared>, pane: PaneId) -> (Publisher, Arc<AgentRuntime>, u64) {
    let publisher = shared.agent_tx.publisher();
    let handle: Arc<dyn AgentPublisher> = Arc::new(publisher.clone());
    let runtime = Arc::new(AgentRuntime::new(
        &handle,
        AgentSpawnConfig::default(),
        None,
    ));
    runtime.set_runner_factory(Box::new(|_| Box::new(|_| Box::pin(std::future::pending()))));
    assert!(runtime.open(pane, spec()));
    let generation = runtime.pane_generation(pane).expect("generation");
    *shared.agent.lock() = Some(Arc::clone(&runtime));
    (publisher, runtime, generation)
}

fn titled(title: &str) -> AgentPaneWire {
    AgentPaneWire {
        title: Some(title.to_owned()),
        ..AgentPaneWire::default()
    }
}

fn published_titles(fixture: &Fixture) -> Vec<String> {
    take_reliable_messages(&fixture.mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::AgentState { pane, state },
                ..
            }) if pane == fixture.pane => state.title,
            _ => None,
        })
        .collect()
}

#[test]
fn retired_runtime_incarnation_messages_are_dropped() {
    let mut fixture = fixture();
    let pane = fixture.pane;
    let (old, retired, generation) = runtime(&fixture.shared, pane);
    old.publish_agent_state(generation, pane, titled("retired state"));
    let (current, live, generation) = runtime(&fixture.shared, pane);
    retired.shutdown();
    fixture.inbox.turn(&fixture.shared);
    assert!(!published_titles(&fixture).contains(&"retired state".to_owned()));
    current.publish_agent_state(generation, pane, titled("current state"));
    fixture.inbox.turn(&fixture.shared);
    assert_eq!(published_titles(&fixture), ["current state"]);
    live.shutdown();
}

#[test]
fn reopened_pane_generation_messages_are_dropped() {
    let mut fixture = fixture();
    let pane = fixture.pane;
    let (publisher, runtime, old_generation) = runtime(&fixture.shared, pane);
    publisher.publish_agent_state(old_generation, pane, titled("retired state"));
    runtime.close(pane);
    assert!(runtime.open(pane, spec()));
    let generation = runtime.pane_generation(pane).unwrap();
    assert_ne!(generation, old_generation);
    fixture.inbox.turn(&fixture.shared);
    assert!(!published_titles(&fixture).contains(&"retired state".to_owned()));
    publisher.publish_agent_state(generation, pane, titled("current state"));
    fixture.inbox.turn(&fixture.shared);
    assert!(published_titles(&fixture).contains(&"current state".to_owned()));
    runtime.shutdown();
}

#[test]
fn agent_inbox_bounds_each_turn_and_keeps_the_next_wake() {
    let mut fixture = fixture();
    let pane = fixture.pane;
    let (publisher, runtime, generation) = runtime(&fixture.shared, pane);
    fixture.inbox.turn(&fixture.shared);
    take_reliable_messages(&fixture.mailbox);
    for index in 0..=DRAIN_BURST {
        publisher.publish_agent_state(generation, pane, titled(&index.to_string()));
    }
    fixture.inbox.turn(&fixture.shared);
    assert_eq!(
        published_titles(&fixture).last().map(String::as_str),
        Some((DRAIN_BURST - 1).to_string().as_str())
    );
    assert!(fixture.shared.agent_tx.pending.load(Ordering::Acquire));
    fixture.inbox.turn(&fixture.shared);
    assert_eq!(published_titles(&fixture), [DRAIN_BURST.to_string()]);
    assert!(!fixture.shared.agent_tx.pending.load(Ordering::Acquire));
    runtime.shutdown();
}

#[test]
fn messages_for_a_pane_that_is_no_longer_a_terminal_are_dropped() {
    let mut fixture = fixture();
    let pane = fixture.pane;
    let (publisher, runtime, generation) = runtime(&fixture.shared, pane);
    fixture.inbox.turn(&fixture.shared);
    take_reliable_messages(&fixture.mailbox);
    fixture
        .shared
        .inner
        .lock()
        .engine
        .state
        .pane_mut(pane)
        .unwrap()
        .kind = PaneKind::Picker {
        inherit_cwd_from: None,
    };
    publisher.publish_agent_state(generation, pane, titled("not a terminal"));
    fixture.inbox.turn(&fixture.shared);
    assert!(published_titles(&fixture).is_empty());
    runtime.shutdown();
}

#[cfg(unix)]
#[test]
fn a_shell_hook_keeps_the_agent_inbox_running() {
    let mut fixture = fixture();
    let pane = fixture.pane;
    let shared = Arc::clone(&fixture.shared);
    let (publisher, runtime, generation) = runtime(&shared, pane);
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-hook",
                ["-g", "pane-mode-changed", "run-shell 'sleep 0.1'"],
            ),
        )
        .expect("parked hook");
    let poll = mio::Poll::new().unwrap();
    let waker = Arc::new(mio::Waker::new(poll.registry(), mio::Token(1)).unwrap());
    let mut timers = timers::LoopTimers::new(&shared, &waker);
    let _jobs = pipe_jobs::Driver::new(&shared);
    fixture.inbox.hooks.push_back(
        PendingHookEvent::live_pane("pane-mode-changed", pane, &shared.inner.lock().engine)
            .expect("live hook pane"),
    );
    fixture.inbox.turn(&shared);
    take_reliable_messages(&fixture.mailbox);
    publisher.publish_agent_state(generation, pane, titled("while hook waits"));
    timers.turn(&shared, &waker).unwrap();
    assert!(!timers.hooks.is_empty());
    fixture.inbox.turn(&shared);
    assert!(published_titles(&fixture).contains(&"while hook waits".to_owned()));
    let deadline = Instant::now() + Duration::from_secs(2);
    while !timers.hooks.is_empty() {
        timers.turn(&shared, &waker).unwrap();
        assert!(Instant::now() < deadline, "hook did not finish");
        fixture.inbox.turn(&shared);
        thread::sleep(Duration::from_millis(1));
    }
    runtime.shutdown();
}
