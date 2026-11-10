use std::{rc::Rc, sync::Arc};

use ala::{
    App, ClickEvent, Context, Entity, Focusable as _, IntoElement, SharedString, Subscription,
    Window, prelude::*, px,
};
use penna::blocks::chat::{
    ASK_GUTTER, AskDetail, AskOption, ask_count, ask_key, ask_leading, ask_options, ask_picked_key,
    ask_question, ask_send, ask_skip, ask_step,
};
use penna::{Label, TextStyle};
use zz_protocol::agent_stream::AgentQuestion;
#[cfg(test)]
use zz_protocol::{
    AgentQuestionAnswer, MAX_AGENT_ANSWER_BYTES, MAX_AGENT_OPTION_BYTES, MAX_AGENT_QUESTION_ANSWERS,
};

use super::ask::{AskParts, ask_hint};
use crate::{
    ActiveTheme as _, Disableable as _, Ground, IconName, Tone,
    input::{InputEvent, InputState},
    v_flex,
};
use zz_client::agent_ask::{KEYED_OPTIONS, question_hints, question_other_key, question_send};

pub use zz_client::agent_question::{
    OTHER_LABEL, QuestionCard, QuestionCardAction, QuestionCardStep,
};

pub struct QuestionCardState {
    pub card: QuestionCard,
    others: Vec<Option<Entity<InputState>>>,
    _subscriptions: Vec<Subscription>,
}

impl QuestionCardState {
    pub fn new<V: 'static>(
        request_id: u64,
        questions: impl Into<Arc<[AgentQuestion]>>,
        window: &mut Window,
        cx: &mut Context<V>,
        on_input: impl Fn(&mut V, usize, &InputEvent, &mut Window, &mut Context<V>) + Clone + 'static,
    ) -> Self {
        let card = QuestionCard::new(request_id, questions);
        let mut subscriptions = Vec::new();
        let others = card
            .questions()
            .iter()
            .enumerate()
            .map(|(index, question)| {
                question.allow_other.then(|| {
                    let placeholder = if question.options.is_empty() {
                        "Type your answer"
                    } else {
                        "Something else"
                    };
                    let input = cx.new(|cx| {
                        InputState::new(window, cx)
                            .auto_grow(1, 8)
                            .submit_on_enter(true)
                            .placeholder(placeholder)
                    });
                    let on_input = on_input.clone();
                    subscriptions.push(cx.subscribe_in(
                        &input,
                        window,
                        move |view, _, event: &InputEvent, window, cx| {
                            on_input(view, index, event, window, cx);
                        },
                    ));
                    input
                })
            })
            .collect();
        Self {
            card,
            others,
            _subscriptions: subscriptions,
        }
    }

    pub const fn request_id(&self) -> u64 {
        self.card.request_id()
    }

    pub fn other_input(&self, question: usize) -> Option<&Entity<InputState>> {
        self.others.get(question).and_then(Option::as_ref)
    }

    pub fn editing(&self, window: &Window, cx: &App) -> bool {
        self.others
            .iter()
            .flatten()
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    pub fn input_changed(&mut self, question: usize, cx: &App) -> bool {
        let Some(input) = self.other_input(question) else {
            return false;
        };
        let text = input.read(cx).value();
        self.card.set_other_text(question, &text)
    }

    fn edit_other(&self, question: usize, window: &mut Window, cx: &mut App) {
        if let Some(input) = self.other_input(question) {
            input.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    pub fn action(
        &mut self,
        action: QuestionCardAction,
        window: &mut Window,
        cx: &mut App,
    ) -> QuestionCardStep {
        let step = self.card.act(action);
        self.follow(step, window, cx)
    }

    pub fn key(
        &mut self,
        key: &str,
        shift: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> QuestionCardStep {
        let step = self.card.key(key, shift);
        self.follow(step, window, cx)
    }

    fn follow(
        &self,
        step: QuestionCardStep,
        window: &mut Window,
        cx: &mut App,
    ) -> QuestionCardStep {
        if let QuestionCardStep::Other(question) = step {
            self.edit_other(question, window, cx);
            return QuestionCardStep::Handled;
        }
        step
    }

    pub fn parts(
        &self,
        id: &str,
        counter: Option<SharedString>,
        enabled: bool,
        on_action: impl Fn(QuestionCardAction, &mut Window, &mut App) + 'static,
        on: Ground,
        cx: &App,
    ) -> AskParts {
        let on_action: Rc<dyn Fn(QuestionCardAction, &mut Window, &mut App)> = Rc::new(on_action);
        let card = &self.card;
        let index = card.focused();
        let count = card.questions().len();
        let act = |action: QuestionCardAction| {
            let on_action = Rc::clone(&on_action);
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                on_action(action, window, cx);
                cx.stop_propagation();
            }
        };
        let Some(question) = card.questions().get(index) else {
            return AskParts {
                body: v_flex().w_full().into_any_element(),
                input: None,
                leading: None,
                trailing: Vec::new(),
                hints: Vec::new(),
                secret: false,
            };
        };
        let header = ask_question(question.question.clone(), on, cx);
        let header = if count > 1 {
            header
                .child(
                    ask_step(
                        SharedString::from(format!("{id}-back")),
                        IconName::ChevronLeft,
                        "Previous question",
                        on,
                    )
                    .disabled(!enabled)
                    .on_click(act(QuestionCardAction::Back)),
                )
                .child(ask_count(format!("{} of {count}", index + 1), on))
                .child(
                    ask_step(
                        SharedString::from(format!("{id}-forward")),
                        IconName::ChevronRight,
                        "Next question",
                        on,
                    )
                    .disabled(!enabled)
                    .on_click(act(QuestionCardAction::Forward)),
                )
        } else {
            header.children(counter.map(|counter| ask_count(counter, on)))
        };
        let choices = question.options.len();
        let rows = (choices > 0).then(|| {
            ask_options().children(question.options.iter().enumerate().map(|(option, choice)| {
                let row = AskOption::new(
                    (SharedString::from(format!("{id}-{index}")), option),
                    choice.label.clone(),
                    on,
                )
                .highlighted(option == card.highlight())
                .enabled(enabled)
                .on_click(act(QuestionCardAction::Pick {
                    question: index,
                    option,
                }));
                let row = match &choice.description {
                    Some(description) => row.detail(description.clone(), AskDetail::Under),
                    None => row,
                };
                let row = if option < KEYED_OPTIONS {
                    row.key((option + 1).to_string())
                } else {
                    row
                };
                if question.multi_select {
                    row.picked(card.is_picked(index, option))
                } else {
                    row
                }
            }))
        });
        let problem = card.problem().map(|problem| {
            Label::new(TextStyle::Small, problem)
                .px(px(ASK_GUTTER))
                .pb(px(8.0))
                .text_color(cx.theme().tone(Tone::Danger))
        });
        let input = self.other_input(index).cloned();
        let key = question_other_key(card).map(|key| SharedString::from(key.to_string()));
        let leading = if question.multi_select && card.other_selected(index) && choices > 0 {
            Some(ask_picked_key(key, on))
        } else {
            key.map(|key| ask_key(key, on))
        }
        .map(|cap| ask_leading(cap).into_any_element());
        let send = question_send(card);
        let hints = question_hints(card).into_iter().map(ask_hint).collect();
        AskParts {
            body: v_flex()
                .w_full()
                .child(header)
                .children(rows)
                .children(problem)
                .into_any_element(),
            input,
            leading,
            trailing: vec![
                ask_skip(SharedString::from(format!("{id}-skip")), on)
                    .disabled(!enabled)
                    .on_click(act(QuestionCardAction::Dismiss))
                    .into_any_element(),
                ask_send(SharedString::from(format!("{id}-send")), send, on)
                    .disabled(!enabled)
                    .on_click(act(QuestionCardAction::Enter))
                    .into_any_element(),
            ],
            hints,
            secret: question.secret,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zz_protocol::agent_stream::AgentQuestionOption;

    fn question(id: &str, labels: &[&str], multi_select: bool) -> AgentQuestion {
        AgentQuestion {
            id: id.to_owned(),
            header: None,
            question: format!("{id}?"),
            options: labels
                .iter()
                .map(|label| AgentQuestionOption {
                    label: (*label).to_owned(),
                    description: None,
                })
                .collect(),
            multi_select,
            allow_other: true,
            secret: false,
        }
    }

    fn answer(id: &str, answers: &[&str]) -> AgentQuestionAnswer {
        AgentQuestionAnswer {
            id: id.to_owned(),
            answers: answers.iter().map(|answer| (*answer).to_owned()).collect(),
        }
    }

    #[test]
    fn one_plain_question_answers_on_the_first_pick() {
        let mut card = QuestionCard::new(4, vec![question("fruit", &["apple", "pear"], false)]);
        assert_eq!(
            card.key("2", false),
            QuestionCardStep::Submit(vec![answer("fruit", &["pear"])])
        );
    }

    #[test]
    fn several_questions_wait_for_every_answer_then_submit_in_order() {
        let mut card = QuestionCard::new(
            4,
            vec![
                question("fruit", &["apple", "pear"], false),
                question("tools", &["saw", "drill", "glue"], true),
            ],
        );
        assert_eq!(card.submit(), QuestionCardStep::Handled);
        assert_eq!(
            card.key("enter", false),
            QuestionCardStep::Stay,
            "Enter reaches the composer, which submits the card"
        );
        assert_eq!(card.key("1", false), QuestionCardStep::Handled);
        assert_eq!(card.focused(), 1, "a single-select pick moves on");
        assert_eq!(card.key("3", false), QuestionCardStep::Handled);
        assert_eq!(card.key("1", false), QuestionCardStep::Handled);
        assert_eq!(card.key("3", false), QuestionCardStep::Handled);
        assert_eq!(card.key("2", false), QuestionCardStep::Handled);
        assert!(card.is_picked(1, 1));
        assert!(
            !card.is_picked(1, 2),
            "a second press clears a multi-select choice"
        );
        assert_eq!(
            card.submit(),
            QuestionCardStep::Submit(vec![
                answer("fruit", &["apple"]),
                answer("tools", &["saw", "drill"])
            ])
        );
        assert_eq!(card.key("left", false), QuestionCardStep::Handled);
        assert_eq!(card.focused(), 0);
        assert_eq!(card.key("tab", true), QuestionCardStep::Handled);
        assert_eq!(card.focused(), 1);
        assert_eq!(card.key("escape", false), QuestionCardStep::Dismiss);
        assert_eq!(card.key("9", false), QuestionCardStep::Stay);
        assert_eq!(card.key("x", false), QuestionCardStep::Stay);
    }

    #[test]
    fn arrows_move_the_highlight_and_enter_picks_it() {
        let mut card =
            QuestionCard::new(4, vec![question("fruit", &["apple", "pear", "fig"], false)]);
        assert_eq!(card.highlight(), 0);
        assert_eq!(card.key("up", false), QuestionCardStep::Handled);
        assert_eq!(card.highlight(), 2, "up from the first option wraps");
        assert_eq!(card.key("down", false), QuestionCardStep::Handled);
        assert_eq!(card.key("down", false), QuestionCardStep::Handled);
        assert_eq!(card.highlight(), 1);
        assert!(card.last_step());
        assert_eq!(
            card.enter(),
            QuestionCardStep::Submit(vec![answer("fruit", &["pear"])])
        );
    }

    #[test]
    fn typed_answers_replace_a_single_choice_and_join_a_multiple_one() {
        let mut card = QuestionCard::new(
            4,
            vec![
                question("fruit", &["apple", "pear"], false),
                question("tools", &["saw", "drill"], true),
            ],
        );
        card.pick(0, 0);
        assert_eq!(card.key("3", false), QuestionCardStep::Other(1));
        card.focus(0);
        assert_eq!(card.key("3", false), QuestionCardStep::Other(0));
        assert!(
            !card.is_picked(0, 0),
            "the typed answer replaces the choice"
        );
        assert!(!card.is_answered(0), "until something is typed");
        assert!(card.set_other_text(0, " quince "));
        card.pick(1, 1);
        assert!(card.set_other_text(1, "chisel"));
        assert_eq!(
            card.answers(),
            Some(vec![
                answer("fruit", &["quince"]),
                answer("tools", &["drill", "chisel"])
            ])
        );
        card.pick(0, 1);
        assert_eq!(
            card.answers().expect("complete")[0],
            answer("fruit", &["pear"])
        );
    }

    #[test]
    fn answers_never_leave_the_wire_limits() {
        let labels = (0..=MAX_AGENT_QUESTION_ANSWERS)
            .map(|index| format!("choice {index}"))
            .collect::<Vec<_>>();
        let labels = labels.iter().map(String::as_str).collect::<Vec<_>>();
        let mut card = QuestionCard::new(4, vec![question("many", &labels, true)]);
        for option in 0..MAX_AGENT_QUESTION_ANSWERS {
            card.pick(0, option);
        }
        let answers = card.answers().expect("the full limit still sends");
        assert!(
            zz_protocol::encode_protocol_message(
                &zz_protocol::ProtocolMessage::AgentAnswerQuestion {
                    pane: zz_protocol::PaneId(1),
                    request_id: 4,
                    answers,
                }
            )
            .is_ok()
        );
        card.select_other(0);
        card.set_other_text(0, "one more");
        assert!(card.is_complete());
        assert!(
            card.problem().is_some(),
            "a typed answer past the limit is refused"
        );
        assert_eq!(card.answers(), None);
        assert_eq!(card.submit(), QuestionCardStep::Handled);

        let mut card = QuestionCard::new(4, vec![question("note", &["short"], false)]);
        card.select_other(0);
        card.set_other_text(0, &"x".repeat(MAX_AGENT_ANSWER_BYTES + 1));
        assert!(card.problem().is_some());
        assert_eq!(card.answers(), None);
        card.set_other_text(0, &"x".repeat(MAX_AGENT_ANSWER_BYTES));
        assert!(card.problem().is_none());
        assert!(card.answers().is_some());

        let mut long_label = question("label", &["fine"], false);
        long_label.options[0].label = "y".repeat(MAX_AGENT_ANSWER_BYTES + 1);
        let mut card = QuestionCard::new(4, vec![long_label]);
        assert_eq!(card.pick(0, 0), QuestionCardStep::Handled);
        assert!(card.problem().is_some());

        let mut long_id = question("id", &["fine"], false);
        long_id.id = "z".repeat(MAX_AGENT_OPTION_BYTES + 1);
        let mut card = QuestionCard::new(4, vec![long_id]);
        assert_eq!(card.pick(0, 0), QuestionCardStep::Handled);
        assert!(card.problem().is_some());

        let many = (0..=MAX_AGENT_QUESTION_ANSWERS)
            .map(|index| question(&format!("q{index}"), &["yes"], false))
            .collect::<Vec<_>>();
        let mut card = QuestionCard::new(4, many);
        for index in 0..=MAX_AGENT_QUESTION_ANSWERS {
            card.pick(index, 0);
        }
        assert!(card.is_complete());
        assert_eq!(card.answers(), None);
    }

    #[test]
    fn a_free_text_question_answers_with_what_was_typed() {
        let mut card = QuestionCard::new(4, vec![question("name", &[], false)]);
        assert_eq!(card.key("1", false), QuestionCardStep::Other(0));
        assert_eq!(card.submit(), QuestionCardStep::Handled);
        card.set_other_text(0, "zz");
        assert_eq!(
            card.submit(),
            QuestionCardStep::Submit(vec![answer("name", &["zz"])])
        );
    }
}
