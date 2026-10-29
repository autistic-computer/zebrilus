use std::{rc::Rc, sync::Arc};

use ala::{
    AnyElement, App, Context, Div, Entity, Focusable as _, FontWeight, IntoElement, SharedString,
    Subscription, Window, div, prelude::*, px,
};
use zz_protocol::agent_stream::AgentQuestion;
#[cfg(test)]
use zz_protocol::{
    AgentQuestionAnswer, MAX_AGENT_ANSWER_BYTES, MAX_AGENT_OPTION_BYTES, MAX_AGENT_QUESTION_ANSWERS,
};

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, PaintState, Tone,
    button::Button,
    check::Check,
    h_flex,
    input::{Input, InputContentType, InputEvent, InputState},
    kbd::Keycap,
    v_flex,
};

use super::presentation::{
    CARD_GAP, CARD_GUTTER, CARD_TITLE_LINE, CardTile, card_button, card_chip, card_close,
    card_divider, card_footer, card_header, card_surface, card_tiles, card_trailing,
};
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
                        "Type your own answer"
                    };
                    let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
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
        let step = match action {
            QuestionCardAction::Pick { question, option } => self.card.pick(question, option),
            QuestionCardAction::Other { question } => self.card.select_other(question),
            QuestionCardAction::Submit => self.card.submit(),
            QuestionCardAction::Dismiss => QuestionCardStep::Dismiss,
        };
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

    pub fn render(
        &self,
        id: &str,
        counter: Option<SharedString>,
        enabled: bool,
        on_action: impl Fn(QuestionCardAction, &mut Window, &mut App) + 'static,
        cx: &App,
    ) -> Div {
        let on_action: Rc<dyn Fn(QuestionCardAction, &mut Window, &mut App)> = Rc::new(on_action);
        let card = &self.card;
        let several = card.questions().len() > 1;
        let problem = card.problem();
        let close = Rc::clone(&on_action);
        let skip = Rc::clone(&on_action);
        let submit = Rc::clone(&on_action);
        let mut trailing = Some(
            counter
                .map(|counter| card_chip(counter).into_any_element())
                .into_iter()
                .chain(std::iter::once(
                    card_close(
                        Button::new(SharedString::from(format!("{id}-close")))
                            .tooltip("Skip")
                            .disabled(!enabled)
                            .on_click(move |_, window, cx| {
                                close(QuestionCardAction::Dismiss, window, cx);
                                cx.stop_propagation();
                            }),
                        cx,
                    )
                    .into_any_element(),
                ))
                .collect::<Vec<_>>(),
        );
        let submittable = enabled && card.is_complete() && problem.is_none();
        let submit_ground =
            cx.theme()
                .paint(Tone::Primary, Emphasis::Solid)
                .ground_at(if submittable {
                    PaintState::Rest
                } else {
                    PaintState::Disabled
                });
        card_surface(cx)
            .children(
                card.questions()
                    .iter()
                    .enumerate()
                    .map(|(index, question)| {
                        self.render_question(
                            id,
                            index,
                            question,
                            several,
                            enabled,
                            trailing.take(),
                            &on_action,
                            cx,
                        )
                    }),
            )
            .when_some(problem.clone(), |card, problem| {
                card.child(
                    div()
                        .px(px(CARD_GUTTER))
                        .pt(px(CARD_GUTTER))
                        .text_size(crate::rems_from_px(12.0))
                        .line_height(px(16.0))
                        .text_color(cx.theme().tone(Tone::Danger))
                        .child(problem),
                )
            })
            .child(
                card_footer()
                    .child(
                        card_button(
                            Button::new(SharedString::from(format!("{id}-skip")))
                                .emphasis(Emphasis::Ghost),
                            cx,
                        )
                        .label("Skip")
                        .disabled(!enabled)
                        .on_click(move |_, window, cx| {
                            skip(QuestionCardAction::Dismiss, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        card_button(
                            Button::new(SharedString::from(format!("{id}-submit")))
                                .tone(Tone::Primary)
                                .emphasis(Emphasis::Solid),
                            cx,
                        )
                        .label("Submit")
                        .child(Keycap::new("↵").on(submit_ground))
                        .disabled(!submittable)
                        .on_click(move |_, window, cx| {
                            submit(QuestionCardAction::Submit, window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_question(
        &self,
        id: &str,
        index: usize,
        question: &AgentQuestion,
        several: bool,
        enabled: bool,
        trailing: Option<Vec<AnyElement>>,
        on_action: &Rc<dyn Fn(QuestionCardAction, &mut Window, &mut App)>,
        cx: &App,
    ) -> AnyElement {
        let card = &self.card;
        let focused = card.focused() == index;
        let keys = enabled && (focused || !several);
        let marker = |option: usize, picked: bool, tile: CardTile| {
            card_trailing().child(if question.multi_select {
                Check::checkbox(picked).on(tile.ground()).into_any_element()
            } else if picked {
                Check::radio(true).on(tile.ground()).into_any_element()
            } else if keys && option < 9 {
                tile.keycap(format!("{}", option + 1)).into_any_element()
            } else {
                div().into_any_element()
            })
        };
        let options = question.options.iter().enumerate().map(|(option, choice)| {
            let on_action = Rc::clone(on_action);
            let picked = card.is_picked(index, option);
            let tile = CardTile::new(Tone::Neutral, enabled, picked, cx);
            tile.render((SharedString::from(format!("{id}-{index}")), option), cx)
                .on_click(move |_, window, cx| {
                    on_action(
                        QuestionCardAction::Pick {
                            question: index,
                            option,
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                })
                .child(
                    v_flex()
                        .min_w_0()
                        .flex_1()
                        .child(choice.label.clone())
                        .when_some(choice.description.clone(), |column, description| {
                            column.child(tile.detail(description))
                        }),
                )
                .child(marker(option, picked, tile))
                .into_any_element()
        });
        let other = self.other_input(index).map(|input| {
            let picked = card.other_selected(index);
            let tile = (!question.options.is_empty())
                .then(|| CardTile::new(Tone::Neutral, enabled, picked, cx));
            let field = Input::new(input)
                .w_full()
                .disabled(!enabled)
                .when_some(tile, |field, tile| field.on(tile.ground()))
                .when(question.secret, |field| {
                    field.content_type(InputContentType::Password)
                });
            let Some(tile) = tile else {
                return field.into_any_element();
            };
            let on_action = Rc::clone(on_action);
            let choice = question.options.len();
            tile.render(
                (SharedString::from(format!("{id}-{index}-other")), choice),
                cx,
            )
            .on_click(move |_, window, cx| {
                on_action(QuestionCardAction::Other { question: index }, window, cx);
                cx.stop_propagation();
            })
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap(px(CARD_GAP))
                    .child(
                        h_flex()
                            .w_full()
                            .gap(px(CARD_GAP))
                            .child(div().min_w_0().flex_1().child(OTHER_LABEL))
                            .child(marker(choice, picked, tile)),
                    )
                    .child(field),
            )
            .into_any_element()
        });
        v_flex()
            .w_full()
            .when(index > 0, |section| section.child(card_divider(cx)))
            .child(
                card_header()
                    .when(index > 0, ala::Styled::pt_0)
                    .when(several && !focused, |header| {
                        header.text_color(cx.theme().foreground.muted())
                    })
                    .when_some(question.header.clone(), |row, header| {
                        row.child(card_chip(header))
                    })
                    .child(div().min_w_0().flex_1().child(question.question.clone()))
                    .when(question.multi_select, |row| {
                        row.child(
                            h_flex()
                                .flex_none()
                                .h(px(CARD_TITLE_LINE))
                                .text_size(crate::rems_from_px(11.0))
                                .font_weight(FontWeight::NORMAL)
                                .text_color(cx.theme().foreground.muted())
                                .child("Pick any"),
                        )
                    })
                    .children(trailing.into_iter().flatten()),
            )
            .child(card_tiles().children(options).children(other))
            .into_any_element()
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
        assert_eq!(card.key("up", false), QuestionCardStep::Handled);
        assert_eq!(card.focused(), 0);
        assert_eq!(card.key("tab", true), QuestionCardStep::Handled);
        assert_eq!(card.focused(), 1);
        assert_eq!(card.key("escape", false), QuestionCardStep::Dismiss);
        assert_eq!(card.key("9", false), QuestionCardStep::Stay);
        assert_eq!(card.key("x", false), QuestionCardStep::Stay);
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
