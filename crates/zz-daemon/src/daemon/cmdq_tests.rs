use super::*;

#[test]
fn interleaved_items_resume_only_their_current_continuation() {
    let first = CommandItem::new(None, String::from("first"));
    let second = CommandItem::new(Some(first.queue), String::from("second"));
    assert_ne!(first.id, second.id);
    assert_eq!(first.queue, second.queue);
    let first_wait = first.wait().unwrap();
    let second_wait = second.wait().unwrap();
    assert!(!first.resume(second_wait));
    assert_eq!(first.state(), State::Waiting(first_wait));
    assert!(second.resume(second_wait));
    assert!(!second.resume(second_wait));
    assert!(first.resume(first_wait));
    let next_wait = first.wait().unwrap();
    assert_ne!(first_wait, next_wait);
    assert!(!first.resume(first_wait));
    assert!(first.resume(next_wait));
    assert_eq!(first.as_str(), "first");
    assert_eq!(second.as_str(), "second");
}

#[test]
fn duplicate_completion_and_cancel_finish_exactly_once() {
    let item = CommandItem::new(None, ());
    let token = item.wait().unwrap();
    let mut finishes = 0;
    for _ in 0..2 {
        if item.finish() {
            finishes += 1;
        }
    }
    assert_eq!(finishes, 1);
    assert_eq!(item.state(), State::Done);
    assert!(!item.resume(token));
    assert!(item.wait().is_none());
}

#[cfg(unix)]
#[test]
fn uncached_listing_parks_between_rows_and_resumes_its_command_queue() {
    let shared = Arc::new(Shared::new(522));
    let (client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    let context = ExecutionContext::default();
    let command = CommandInvocation::new(
        "list-keys",
        ["-F", "#{key_table} #{key_string} #{key_command}"],
    );
    let mut task = wait_queue::CommandTask::new(
        &shared,
        client,
        ClientKind::Command,
        &context,
        522,
        &command,
        false,
    )
    .unwrap_or_else(|_| panic!("listing task"));
    assert!(matches!(task.run(true), wait_queue::Progress::Waiting));
    assert!(!task.ready());
    shared.terminal_requests.turn(&shared);
    assert!(!task.ready());
    let mut turns = 1;
    while !task.ready() {
        assert!(turns < 20);
        shared.terminal_requests.turn(&shared);
        turns += 1;
    }
    assert!(turns > 1);
    assert!(matches!(task.run(true), wait_queue::Progress::Done));
    let response = task.finish().0;
    assert!(matches!(response, CommandResponse::Success { .. }));
    shared.request_shutdown();
}
