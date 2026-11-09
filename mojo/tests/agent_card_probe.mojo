from std.sys import argv
from browser_fixture import Source
from agent_fixture import AgentCase, bit, number, read_case, some, unsigned
from zzmojo.agent_text import text_join, text_slice, text_split
from zzmojo.agent_chat import ChatChoice, ChatQuestion, QuestionAnswer
from zzmojo.agent_question import (
    QUESTION_DISMISS,
    QUESTION_HANDLED,
    QUESTION_OTHER,
    QUESTION_SUBMIT,
    QuestionCard,
    QuestionStep,
)
from zzmojo.agent_settings import (
    AGENT_CLAUDE,
    AGENT_CODEX,
    LensSettings,
    SETTER_KEEP,
    SETTER_LIST,
    SettingChoice,
    composer_setter,
    fork_chat_command,
    real_session,
    setter,
    settings_from_json,
    short_folder,
    side_chat_command,
    source_message,
)
from zzmojo.agent_completion import (
    AgentCommand,
    CommandCompletion,
    active_command_hint,
    command_label,
    completion_query,
    completion_score,
    composer_completions,
    meaningful_command_description,
    pane_commands,
    rendered_error,
    setting_completions,
)

comptime FIELD = "\x1f"
comptime RECORD = "\x1e"
comptime ONE = "\x1d"
comptime TWO = "\x1c"
comptime THREE = "\x1b"


def listed(parts: List[String], separator: String) -> String:
    var out = String(len(parts))
    for part in parts:
        out += separator
        out += part
    return out^


def unlisted(text: String, separator: String) -> List[String]:
    var parts = text_split(text, separator)
    var found = List[String]()
    for index in range(1, len(parts)):
        found.append(parts[index].copy())
    return found^


def optional(text: String) -> Optional[String]:
    if text.byte_length() == 0 or text.as_bytes()[0] != 83:
        return None
    return Optional[String](text_slice(text, 1, text.byte_length()))


def same(op: String, index: Int, actual: String, expected: String) raises:
    if actual == expected:
        return
    var shared = min(actual.byte_length(), expected.byte_length())
    var at = 0
    while at < shared and actual.as_bytes()[at] == expected.as_bytes()[at]:
        at += 1
    var start = max(at - 60, 0)
    raise Error(
        String(
            op,
            " case ",
            index,
            " differs at byte ",
            at,
            "\n  actual:   ",
            repr(text_slice(actual, start, at + 120)),
            "\n  expected: ",
            repr(text_slice(expected, start, at + 120)),
        )
    )


def answers_text(answers: List[QuestionAnswer]) -> String:
    var parts = List[String]()
    for answer in answers:
        parts.append(String(answer.question, TWO, listed(answer.answers, THREE)))
    return listed(parts, ONE)


def step_text(step: QuestionStep) -> String:
    if step.kind == QUESTION_HANDLED:
        return "handled"
    if step.kind == QUESTION_OTHER:
        return String("other", step.question)
    if step.kind == QUESTION_SUBMIT:
        return String("submit", answers_text(step.answers))
    if step.kind == QUESTION_DISMISS:
        return "dismiss"
    return "stay"


def card_state(card: QuestionCard, result: String) -> String:
    var state = List[String]()
    for index in range(len(card.questions)):
        var row = String()
        for option in range(len(card.questions[index].options)):
            row += bit(card.is_picked(index, option))
        row += "/"
        row += bit(card.other_selected(index))
        row += bit(card.is_answered(index))
        state.append(row^)
    var parts: List[String] = [
        result.copy(),
        String(card.focused),
        text_join(state, ","),
        bit(card.is_complete()),
        some(card.problem()),
    ]
    return text_join(parts, FIELD)


def card(item: AgentCase) -> String:
    var questions = List[ChatQuestion]()
    for entry in unlisted(item.fields[0], RECORD):
        var parts = text_split(entry, FIELD)
        var options = List[ChatChoice]()
        for label in unlisted(parts[3], ONE):
            options.append(ChatChoice(label.copy(), None))
        questions.append(
            ChatQuestion(
                parts[0].copy(),
                None,
                String(),
                options^,
                parts[1] == "1",
                parts[2] == "1",
                False,
            )
        )
    var card = QuestionCard(7, questions^)
    var out = String()
    for entry in unlisted(item.fields[1], RECORD):
        var parts = text_split(entry, FIELD)
        var result: String
        if parts[0] == "pick":
            result = step_text(card.pick(number(parts[1]), number(parts[2])))
        elif parts[0] == "other":
            result = step_text(card.select_other(number(parts[1])))
        elif parts[0] == "text":
            result = bit(card.set_other_text(number(parts[1]), parts[2]))
        elif parts[0] == "key":
            result = step_text(card.key(parts[1], parts[2] == "1"))
        elif parts[0] == "focus":
            result = bit(card.focus(number(parts[1])))
        else:
            result = step_text(card.submit())
        out += card_state(card, result)
        out += RECORD
    var answers = card.answers()
    if answers:
        out += String("S", answers_text(answers.value()))
    else:
        out += "N"
    return out^


def choices_text(choices: List[SettingChoice]) -> String:
    var parts = List[String]()
    for choice in choices:
        parts.append(String(choice.value, TWO, choice.name))
    return listed(parts, ONE)


def settings_text(settings: Optional[LensSettings]) -> String:
    if not settings:
        return "N"
    var parts: List[String] = [
        settings.value().model.copy(),
        settings.value().model_label.copy(),
        choices_text(settings.value().models),
        settings.value().effort.copy(),
        choices_text(settings.value().efforts),
        settings.value().effort_name(),
        settings.value().summary(),
    ]
    return String("S", text_join(parts, FIELD))


def parsed_settings(text: String) -> Optional[LensSettings]:
    var source = optional(text)
    if not source:
        return None
    return settings_from_json(source.value())


def setter_text(text: String) -> String:
    var found = setter(text)
    if not found:
        return "N"
    return String("S", found.value().option, FIELD, some(found.value().value))


def composer_text(item: AgentCase) -> String:
    var found = composer_setter(item.fields[0], parsed_settings(item.fields[1]))
    if not found:
        return "N"
    if found.value().kind == SETTER_LIST:
        return String("L", found.value().option)
    if found.value().kind == SETTER_KEEP:
        return "K"
    return String("V", found.value().option, FIELD, found.value().value)


def provider(text: String) -> UInt8:
    return AGENT_CLAUDE if text == "1" else AGENT_CODEX


def commands(text: String) -> List[AgentCommand]:
    var found = List[AgentCommand]()
    for entry in unlisted(text, RECORD):
        var parts = text_split(entry, FIELD)
        found.append(
            AgentCommand(
                parts[0].copy(), parts[1].copy(), optional(parts[2]), parts[3] == "1"
            )
        )
    return found^


def commands_text(commands: List[AgentCommand]) -> String:
    var parts = List[String]()
    for command in commands:
        var row: List[String] = [
            command.name.copy(),
            command.description.copy(),
            some(command.input_hint),
            bit(command.skill),
        ]
        parts.append(text_join(row, FIELD))
    return listed(parts, RECORD)


def completions_text(completions: List[CommandCompletion]) -> String:
    var parts = List[String]()
    for completion in completions:
        var row: List[String] = [
            completion.command.name.copy(),
            completion.command.description.copy(),
            some(completion.command.input_hint),
            bit(completion.command.skill),
            String(completion.start),
            String(completion.end),
            completion.insertion(),
            bit(completion.submits()),
        ]
        parts.append(text_join(row, FIELD))
    return listed(parts, RECORD)


def query_text(item: AgentCase) -> String:
    var found = completion_query(item.fields[0], number(item.fields[1]))
    if not found:
        return "N"
    return String(
        "S",
        found.value().needle,
        FIELD,
        found.value().start,
        FIELD,
        found.value().end,
    )


def setting_text(item: AgentCase) -> String:
    var settings = settings_from_json(item.fields[2])
    var found = setting_completions(
        item.fields[0], number(item.fields[1]), settings.value()
    )
    if not found:
        return "N"
    return String("S", completions_text(found.value()))


def score_text(item: AgentCase) -> String:
    var score = completion_score(item.fields[0], item.fields[1])
    return "N" if score < 0 else String(score)


def run(item: AgentCase) raises -> String:
    if item.op == "card":
        return card(item)
    if item.op == "settings":
        return settings_text(settings_from_json(item.fields[0]))
    if item.op == "folder":
        return short_folder(item.fields[0], optional(item.fields[1]))
    if item.op == "setter":
        return setter_text(item.fields[0])
    if item.op == "composer":
        return composer_text(item)
    if item.op == "source":
        return some(source_message(item.fields[0]))
    if item.op == "real":
        return bit(real_session(item.fields[0]))
    if item.op == "fork":
        return text_join(
            fork_chat_command(
                provider(item.fields[0]),
                item.fields[1],
                item.fields[2],
                optional(item.fields[3]),
                parsed_settings(item.fields[4]),
            ),
            FIELD,
        )
    if item.op == "side":
        return text_join(
            side_chat_command(
                provider(item.fields[0]),
                item.fields[1],
                item.fields[2],
                unsigned(item.fields[3]),
                parsed_settings(item.fields[4]),
            ),
            FIELD,
        )
    if item.op == "error":
        return rendered_error(item.fields[0])
    if item.op == "commands":
        return commands_text(
            pane_commands(commands(item.fields[0]), item.fields[1] == "1")
        )
    if item.op == "label":
        return command_label(item.fields[0])
    if item.op == "meaningful":
        return some(meaningful_command_description(item.fields[0]))
    if item.op == "score":
        return score_text(item)
    if item.op == "query":
        return query_text(item)
    if item.op == "complete":
        return completions_text(
            composer_completions(
                item.fields[0],
                number(item.fields[1]),
                commands(item.fields[2]),
                parsed_settings(item.fields[3]),
            )
        )
    if item.op == "setting":
        return setting_text(item)
    if item.op == "hint":
        return some(active_command_hint(item.fields[0], commands(item.fields[1])))
    raise Error(String("unknown op ", item.op))


def main() raises:
    var arguments = argv()
    var bytes: List[UInt8]
    with open(String(arguments[1]), "r") as file:
        bytes = file.read_bytes()
    var source = Source(bytes^)
    var total = Int(source.uint(4))
    var names: List[String] = [
        "card",
        "settings",
        "folder",
        "setter",
        "composer",
        "source",
        "real",
        "fork",
        "side",
        "error",
        "commands",
        "label",
        "meaningful",
        "score",
        "query",
        "complete",
        "setting",
        "hint",
    ]
    var counts = List[Int]()
    for _ in range(len(names)):
        counts.append(0)
    for index in range(total):
        var item = read_case(source)
        same(item.op, index, run(item), item.expected)
        for slot in range(len(names)):
            if names[slot] == item.op:
                counts[slot] += 1
    var out = String()
    for slot in range(len(names)):
        if slot > 0:
            out += " "
        out += String(names[slot], "=", counts[slot])
    print(out)
