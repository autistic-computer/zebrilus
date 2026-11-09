from .agent_chat import ChatQuestion, QuestionAnswer
from .agent_text import text_trim

comptime QUESTION_STAY: UInt8 = 0
comptime QUESTION_HANDLED: UInt8 = 1
comptime QUESTION_OTHER: UInt8 = 2
comptime QUESTION_SUBMIT: UInt8 = 3
comptime QUESTION_DISMISS: UInt8 = 4

comptime QUESTION_OTHER_LABEL = "Other"
comptime QUESTION_MAX_ID_BYTES = 4096
comptime QUESTION_MAX_ANSWERS = 32
comptime QUESTION_MAX_ANSWER_BYTES = 16384


@fieldwise_init
struct QuestionStep(Copyable, Movable):
    var kind: UInt8
    var question: Int
    var answers: List[QuestionAnswer]


def _step(kind: UInt8, question: Int = 0) -> QuestionStep:
    return QuestionStep(kind, question, List[QuestionAnswer]())


def _digit(key: String) -> Int:
    var count = key.byte_length()
    var index = 0
    if count > 0 and key.as_bytes()[0] == 43:
        index = 1
    if index >= count:
        return -1
    var value = 0
    while index < count:
        var byte = Int(key.as_bytes()[index])
        if byte < 48 or byte > 57:
            return -1
        value = value * 10 + byte - 48
        if value > 9:
            return -1
        index += 1
    return value


struct QuestionCard(Copyable, Movable):
    var request_id: UInt64
    var questions: List[ChatQuestion]
    var picks: List[List[Bool]]
    var other: List[Bool]
    var other_text: List[String]
    var focused: Int

    def __init__(out self, request_id: UInt64, var questions: List[ChatQuestion]):
        self.request_id = request_id
        self.picks = List[List[Bool]]()
        self.other = List[Bool]()
        self.other_text = List[String]()
        for question in questions:
            var row = List[Bool]()
            for _ in range(len(question.options)):
                row.append(False)
            self.picks.append(row^)
            self.other.append(False)
            self.other_text.append(String())
        self.questions = questions^
        self.focused = 0

    def is_picked(self, question: Int, option: Int) -> Bool:
        if question < 0 or question >= len(self.picks):
            return False
        if option < 0 or option >= len(self.picks[question]):
            return False
        return self.picks[question][option]

    def other_selected(self, question: Int) -> Bool:
        if question < 0 or question >= len(self.other):
            return False
        return self.other[question]

    def other_value(self, question: Int) -> String:
        if question < 0 or question >= len(self.other_text):
            return String()
        return self.other_text[question].copy()

    def submits_on_pick(self) -> Bool:
        return len(self.questions) == 1 and not self.questions[0].multi_select

    def focus(mut self, question: Int) -> Bool:
        if (
            question < 0
            or question >= len(self.questions)
            or self.focused == question
        ):
            return False
        self.focused = question
        return True

    def clear(mut self, question: Int):
        for index in range(len(self.picks[question])):
            self.picks[question][index] = False

    def pick(mut self, question: Int, option: Int) -> QuestionStep:
        if question < 0 or question >= len(self.questions):
            return _step(QUESTION_STAY)
        if option < 0 or option >= len(self.questions[question].options):
            return _step(QUESTION_STAY)
        self.focused = question
        if self.questions[question].multi_select:
            self.picks[question][option] = not self.picks[question][option]
            return _step(QUESTION_HANDLED)
        self.clear(question)
        self.picks[question][option] = True
        self.other[question] = False
        if self.submits_on_pick():
            return self.submit()
        if question + 1 < len(self.questions):
            self.focused = question + 1
        return _step(QUESTION_HANDLED)

    def select_other(mut self, question: Int) -> QuestionStep:
        if question < 0 or question >= len(self.questions):
            return _step(QUESTION_STAY)
        if not self.questions[question].allow_other:
            return _step(QUESTION_STAY)
        self.focused = question
        if self.questions[question].multi_select:
            self.other[question] = not self.other[question]
        else:
            self.other[question] = True
            self.clear(question)
        if self.other[question]:
            return _step(QUESTION_OTHER, question)
        return _step(QUESTION_HANDLED)

    def set_other_text(mut self, question: Int, text: String) -> Bool:
        if question < 0 or question >= len(self.questions):
            return False
        if not self.questions[question].allow_other:
            return False
        if self.other_text[question] == text:
            return False
        self.other_text[question] = text.copy()
        self.focused = question
        if text_trim(text).byte_length() > 0 and not self.other[question]:
            self.other[question] = True
            if not self.questions[question].multi_select:
                self.clear(question)
        return True

    def is_answered(self, question: Int) -> Bool:
        if question < 0 or question >= len(self.questions):
            return False
        for picked in self.picks[question]:
            if picked:
                return True
        return (
            self.other[question]
            and text_trim(self.other_text[question]).byte_length() > 0
        )

    def question_answers(self, question: Int) -> List[String]:
        var answers = List[String]()
        for index in range(len(self.questions[question].options)):
            if self.picks[question][index]:
                answers.append(self.questions[question].options[index].label.copy())
        if self.other[question]:
            var text = text_trim(self.other_text[question])
            if text.byte_length() > 0:
                answers.append(text^)
        return answers^

    def problem(self) -> Optional[String]:
        var broken = len(self.questions) > QUESTION_MAX_ANSWERS
        for question in self.questions:
            if question.id.byte_length() > QUESTION_MAX_ID_BYTES:
                broken = True
        if broken:
            return Optional[String](
                "zz cannot send answers to this card. Dismiss it."
            )
        for index in range(len(self.questions)):
            var answers = self.question_answers(index)
            if len(answers) > QUESTION_MAX_ANSWERS:
                return Optional[String](
                    "Choose at most 32 answers to one question."
                )
            for answer in answers:
                if answer.byte_length() > QUESTION_MAX_ANSWER_BYTES:
                    return Optional[String](
                        "An answer is over 16 KB. Shorten it."
                    )
        return None

    def is_complete(self) -> Bool:
        if len(self.questions) == 0:
            return False
        for index in range(len(self.questions)):
            if not self.is_answered(index):
                return False
        return True

    def answers(self) -> Optional[List[QuestionAnswer]]:
        if not self.is_complete() or Bool(self.problem()):
            return None
        var found = List[QuestionAnswer]()
        for index in range(len(self.questions)):
            found.append(
                QuestionAnswer(
                    self.questions[index].id.copy(), self.question_answers(index)
                )
            )
        return Optional[List[QuestionAnswer]](found^)

    def submit(self) -> QuestionStep:
        var found = self.answers()
        if found:
            return QuestionStep(QUESTION_SUBMIT, 0, found.take())
        return _step(QUESTION_HANDLED)

    def move_focus(mut self, forward: Bool) -> QuestionStep:
        var count = len(self.questions)
        if count > 1:
            if forward:
                self.focused = (self.focused + 1) % count
            else:
                self.focused = (self.focused + count - 1) % count
        return _step(QUESTION_HANDLED)

    def key(mut self, key: String, shift: Bool) -> QuestionStep:
        if key == "escape":
            return _step(QUESTION_DISMISS)
        if key == "tab":
            return self.move_focus(not shift)
        if key == "down":
            return self.move_focus(True)
        if key == "up":
            return self.move_focus(False)
        var digit = _digit(key)
        if digit < 1:
            return _step(QUESTION_STAY)
        var choices = 0
        if self.focused < len(self.questions):
            choices = len(self.questions[self.focused].options)
        if digit - 1 < choices:
            return self.pick(self.focused, digit - 1)
        if digit - 1 == choices:
            return self.select_other(self.focused)
        return _step(QUESTION_STAY)
