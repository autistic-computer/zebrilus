use std::sync::Arc;

use zz_protocol::{
    AgentQuestionAnswer, MAX_AGENT_ANSWER_BYTES, MAX_AGENT_OPTION_BYTES,
    MAX_AGENT_QUESTION_ANSWERS, agent_stream::AgentQuestion,
};

pub const OTHER_LABEL: &str = "Other";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuestionCardStep {
    Stay,
    Handled,
    Other(usize),
    Submit(Vec<AgentQuestionAnswer>),
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionCardAction {
    Pick { question: usize, option: usize },
    Other { question: usize },
    Back,
    Forward,
    Enter,
    Submit,
    Dismiss,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionCard {
    request_id: u64,
    questions: Arc<[AgentQuestion]>,
    picks: Vec<Vec<bool>>,
    other: Vec<bool>,
    other_text: Vec<String>,
    focused: usize,
    highlight: usize,
}

impl QuestionCard {
    pub fn new(request_id: u64, questions: impl Into<Arc<[AgentQuestion]>>) -> Self {
        let questions = questions.into();
        Self {
            request_id,
            picks: questions
                .iter()
                .map(|question| vec![false; question.options.len()])
                .collect(),
            other: vec![false; questions.len()],
            other_text: vec![String::new(); questions.len()],
            questions,
            focused: 0,
            highlight: 0,
        }
    }

    pub const fn request_id(&self) -> u64 {
        self.request_id
    }

    pub fn questions(&self) -> &[AgentQuestion] {
        &self.questions
    }

    pub const fn focused(&self) -> usize {
        self.focused
    }

    pub const fn highlight(&self) -> usize {
        self.highlight
    }

    pub fn last_step(&self) -> bool {
        (0..self.questions.len()).all(|index| index == self.focused || self.is_answered(index))
    }

    fn set_page(&mut self, question: usize) {
        if self.focused != question {
            self.focused = question;
            self.highlight = self
                .picks
                .get(question)
                .and_then(|picks| picks.iter().position(|picked| *picked))
                .unwrap_or(0);
        }
    }

    pub fn is_picked(&self, question: usize, option: usize) -> bool {
        self.picks
            .get(question)
            .and_then(|picks| picks.get(option))
            .copied()
            .unwrap_or(false)
    }

    pub fn other_selected(&self, question: usize) -> bool {
        self.other.get(question).copied().unwrap_or(false)
    }

    #[must_use]
    pub fn other_text(&self, question: usize) -> &str {
        self.other_text.get(question).map_or("", String::as_str)
    }

    fn submits_on_pick(&self) -> bool {
        matches!(&*self.questions, [only] if !only.multi_select)
    }

    pub fn focus(&mut self, question: usize) -> bool {
        if question >= self.questions.len() || self.focused == question {
            return false;
        }
        self.set_page(question);
        true
    }

    pub fn pick(&mut self, question: usize, option: usize) -> QuestionCardStep {
        let Some(multi_select) = self.questions.get(question).map(|q| q.multi_select) else {
            return QuestionCardStep::Stay;
        };
        let Some(picks) = self
            .picks
            .get_mut(question)
            .filter(|picks| option < picks.len())
        else {
            return QuestionCardStep::Stay;
        };
        if multi_select {
            picks[option] = !picks[option];
        } else {
            for (index, picked) in picks.iter_mut().enumerate() {
                *picked = index == option;
            }
            self.other[question] = false;
        }
        self.set_page(question);
        self.highlight = option;
        if multi_select {
            return QuestionCardStep::Handled;
        }
        if self.submits_on_pick() {
            return self.submit();
        }
        if question + 1 < self.questions.len() {
            self.set_page(question + 1);
        }
        QuestionCardStep::Handled
    }

    pub fn select_other(&mut self, question: usize) -> QuestionCardStep {
        let Some(asked) = self.questions.get(question).filter(|q| q.allow_other) else {
            return QuestionCardStep::Stay;
        };
        let multi_select = asked.multi_select;
        self.set_page(question);
        if multi_select {
            self.other[question] = !self.other[question];
        } else {
            self.other[question] = true;
            self.picks[question].fill(false);
        }
        if self.other[question] {
            QuestionCardStep::Other(question)
        } else {
            QuestionCardStep::Handled
        }
    }

    pub fn set_other_text(&mut self, question: usize, text: &str) -> bool {
        let Some(multi_select) = self
            .questions
            .get(question)
            .filter(|q| q.allow_other)
            .map(|q| q.multi_select)
        else {
            return false;
        };
        if self.other_text[question] == text {
            return false;
        }
        text.clone_into(&mut self.other_text[question]);
        self.set_page(question);
        if !text.trim().is_empty() && !self.other[question] {
            self.other[question] = true;
            if !multi_select {
                self.picks[question].fill(false);
            }
        }
        true
    }

    pub fn picked(&self, question: usize) -> usize {
        if question < self.questions.len() {
            self.question_answers(question).len()
        } else {
            0
        }
    }

    pub fn is_answered(&self, question: usize) -> bool {
        self.picks
            .get(question)
            .is_some_and(|picks| picks.iter().any(|picked| *picked))
            || (self.other_selected(question) && !self.other_text[question].trim().is_empty())
    }

    fn question_answers(&self, index: usize) -> Vec<String> {
        self.questions[index]
            .options
            .iter()
            .zip(&self.picks[index])
            .filter(|(_, picked)| **picked)
            .map(|(option, _)| option.label.clone())
            .chain(
                self.other[index]
                    .then(|| self.other_text[index].trim().to_owned())
                    .filter(|text| !text.is_empty()),
            )
            .collect()
    }

    pub fn problem(&self) -> Option<String> {
        if self.questions.len() > MAX_AGENT_QUESTION_ANSWERS
            || self
                .questions
                .iter()
                .any(|question| question.id.len() > MAX_AGENT_OPTION_BYTES)
        {
            return Some("zz cannot send answers to this card. Dismiss it.".to_owned());
        }
        (0..self.questions.len()).find_map(|index| {
            let answers = self.question_answers(index);
            if answers.len() > MAX_AGENT_QUESTION_ANSWERS {
                Some(format!(
                    "Choose at most {MAX_AGENT_QUESTION_ANSWERS} answers to one question."
                ))
            } else if answers
                .iter()
                .any(|answer| answer.len() > MAX_AGENT_ANSWER_BYTES)
            {
                Some(format!(
                    "An answer is over {} KB. Shorten it.",
                    MAX_AGENT_ANSWER_BYTES / 1024
                ))
            } else {
                None
            }
        })
    }

    pub fn is_complete(&self) -> bool {
        !self.questions.is_empty() && (0..self.questions.len()).all(|index| self.is_answered(index))
    }

    pub fn answers(&self) -> Option<Vec<AgentQuestionAnswer>> {
        (self.is_complete() && self.problem().is_none()).then(|| {
            self.questions
                .iter()
                .enumerate()
                .map(|(index, question)| AgentQuestionAnswer {
                    id: question.id.clone(),
                    answers: self.question_answers(index),
                })
                .collect()
        })
    }

    pub fn submit(&self) -> QuestionCardStep {
        self.answers()
            .map_or(QuestionCardStep::Handled, QuestionCardStep::Submit)
    }

    pub fn page(&mut self, forward: bool) -> QuestionCardStep {
        let count = self.questions.len();
        if count > 1 {
            self.set_page(if forward {
                (self.focused + 1) % count
            } else {
                (self.focused + count - 1) % count
            });
        }
        QuestionCardStep::Handled
    }

    fn choices(&self) -> usize {
        self.questions
            .get(self.focused)
            .map_or(0, |asked| asked.options.len())
    }

    fn move_highlight(&mut self, forward: bool) -> QuestionCardStep {
        let choices = self.choices();
        if choices == 0 {
            return QuestionCardStep::Stay;
        }
        self.highlight = if forward {
            (self.highlight + 1) % choices
        } else {
            (self.highlight + choices - 1) % choices
        };
        QuestionCardStep::Handled
    }

    pub fn enter(&mut self) -> QuestionCardStep {
        let question = self.focused;
        let count = self.questions.len();
        if !self.is_answered(question) {
            return if self.highlight < self.choices() {
                self.pick(question, self.highlight)
            } else {
                self.select_other(question)
            };
        }
        if self.is_complete() {
            return self.submit();
        }
        if let Some(next) = (1..count)
            .map(|offset| (question + offset) % count)
            .find(|index| !self.is_answered(*index))
        {
            self.set_page(next);
        }
        QuestionCardStep::Handled
    }

    pub fn act(&mut self, action: QuestionCardAction) -> QuestionCardStep {
        match action {
            QuestionCardAction::Pick { question, option } => self.pick(question, option),
            QuestionCardAction::Other { question } => self.select_other(question),
            QuestionCardAction::Back => self.page(false),
            QuestionCardAction::Forward => self.page(true),
            QuestionCardAction::Enter => self.enter(),
            QuestionCardAction::Submit => self.submit(),
            QuestionCardAction::Dismiss => QuestionCardStep::Dismiss,
        }
    }

    pub fn key(&mut self, key: &str, shift: bool) -> QuestionCardStep {
        match key {
            "escape" => QuestionCardStep::Dismiss,
            "tab" => self.page(!shift),
            "right" => self.page(true),
            "left" => self.page(false),
            "down" => self.move_highlight(true),
            "up" => self.move_highlight(false),
            key => match key.parse::<usize>() {
                Ok(digit @ 1..=9) => {
                    let question = self.focused;
                    let choices = self.choices();
                    match digit - 1 {
                        option if option < choices => self.pick(question, option),
                        option if option == choices => self.select_other(question),
                        _ => QuestionCardStep::Stay,
                    }
                }
                _ => QuestionCardStep::Stay,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use zz_protocol::agent_stream::AgentQuestionOption;

    use super::*;
    use crate::agent_ask::{Hint, question_hints};

    #[test]
    fn a_pick_any_hint_line_counts_what_is_picked() {
        let question = AgentQuestion {
            id: "checks".to_owned(),
            header: Some("Checks".to_owned()),
            question: "Which checks should run?".to_owned(),
            options: ["test", "clippy", "fmt"]
                .map(|label| AgentQuestionOption {
                    label: label.to_owned(),
                    description: None,
                })
                .to_vec(),
            multi_select: true,
            allow_other: true,
            secret: false,
        };
        let mut card = QuestionCard::new(1, vec![question]);
        assert_eq!(
            question_hints(&card),
            [
                Hint::new("Pick any"),
                Hint::new("1 to 4 toggle"),
                Hint::new("↑↓ move"),
                Hint::new("↵ send"),
                Hint::new("esc skip"),
            ]
        );
        card.pick(0, 0);
        card.pick(0, 2);
        assert_eq!(question_hints(&card)[0], Hint::strong("2 picked"));
        card.select_other(0);
        assert_eq!(question_hints(&card)[0], Hint::strong("2 picked"));
        card.set_other_text(0, "deny");
        assert_eq!(question_hints(&card)[0], Hint::strong("3 picked"));
    }
}
