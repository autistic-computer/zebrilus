use agent_client_protocol::v1::schema::{ContentBlock, ContentChunk, ImageContent, TextContent};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use super::test_support::*;
use super::*;

#[test]
fn a_replayed_user_image_returns_to_the_transcript() {
    let mut thread = thread();
    let attachment = attachment();
    let sent = ContentBlock::Image(ImageContent::new(
        BASE64.encode(&attachment.bytes),
        attachment.format.mime_type(),
    ));

    thread.apply_update(SessionUpdate::UserMessageChunk(ContentChunk::new(sent)));

    let Some(AgentThreadEntry::User {
        markdown, images, ..
    }) = thread.transcript.entries().first()
    else {
        panic!("the replayed chunk should open a user entry");
    };
    assert!(
        markdown.is_empty(),
        "an image should not leave a text placeholder behind"
    );
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].bytes, PNG_PIXEL);
}

#[test]
fn a_replayed_data_uri_becomes_an_image_rather_than_a_link() {
    let mut thread = thread();
    let replayed = format!(
        "hi can you read this image properly?[@image](data:image/png;base64,{})",
        BASE64.encode(PNG_PIXEL)
    );
    thread.apply_update(SessionUpdate::UserMessageChunk(ContentChunk::new(
        ContentBlock::Text(TextContent::new(replayed)),
    )));

    let Some(AgentThreadEntry::User {
        markdown, images, ..
    }) = thread.transcript.entries().first()
    else {
        panic!("the replayed chunk should open a user entry");
    };
    assert_eq!(
        markdown, "hi can you read this image properly?",
        "the link should leave with the image, not linger as `[@image]`"
    );
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].bytes, PNG_PIXEL);
}

#[test]
fn text_that_only_looks_like_an_attachment_is_left_alone() {
    let (kept, images) = split_inline_images("see data:image/png;base64,not-base64!!");
    assert!(images.is_empty());
    assert_eq!(kept, "see data:image/png;base64,not-base64!!");

    let (kept, images) = split_inline_images("just a sentence");
    assert!(images.is_empty());
    assert_eq!(kept, "just a sentence");
}

#[test]
fn an_agent_that_takes_no_images_refuses_them_before_the_turn_starts() {
    let mut thread = thread();
    thread.connection = AgentConnectionState::Ready;

    assert!(
        !thread.session_capabilities.images,
        "an agent is assumed to take no images until it advertises them"
    );
    assert!(
        thread.prompt_refusal(false).is_none(),
        "text is always fine"
    );
    assert_eq!(
        thread.prompt_refusal(true).as_deref(),
        Some("this agent does not accept images")
    );

    thread.session_capabilities.images = true;
    assert!(thread.prompt_refusal(true).is_none());

    thread.connection = AgentConnectionState::Starting;
    assert!(thread.prompt_refusal(true).is_some());
}
