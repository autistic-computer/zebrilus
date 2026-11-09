from .agent_json import Json, JSON_ARRAY, json_text
from .agent_text import (
    text_slice,
    text_trim,
)
from .palette_text import palette_space, scalar_width
from .palette_verbs import palette_quote

comptime SETTING_MODEL = "model"
comptime SETTING_EFFORT = "effort"
comptime SIDE_CHAT_WIDTH = "46%"
comptime SIDE_CHAT_HEIGHT = "72%"

comptime AGENT_CODEX: UInt8 = 0
comptime AGENT_CLAUDE: UInt8 = 1

comptime SETTER_LIST: UInt8 = 0
comptime SETTER_KEEP: UInt8 = 1
comptime SETTER_SET: UInt8 = 2


@fieldwise_init
struct SettingChoice(Copyable, Movable):
    var value: String
    var name: String


@fieldwise_init
struct Setter(Copyable, Movable):
    var option: String
    var value: Optional[String]


@fieldwise_init
struct ComposerSetter(Copyable, Movable):
    var kind: UInt8
    var option: String
    var value: String


def _choices(json: Json, option: Int) -> List[SettingChoice]:
    var found = List[SettingChoice]()
    var options = json.last(option, "options")
    if options < 0 or json.kind(options) != JSON_ARRAY:
        return found^
    for choice in json.children(options):
        var value = json.last_text(choice, "value")
        if not value:
            continue
        var name = json.last_text(choice, "name")
        found.append(
            SettingChoice(value.value().copy(), name.value().copy() if name else String())
        )
    return found^


def _find(json: Json, options: List[Int], id: String) -> Int:
    for option in options:
        var found = json.last_text(option, "id")
        if found and found.value() == id:
            return option
    return -1


def _current(json: Json, option: Int) -> String:
    var found = json.last_text(option, "currentValue")
    return found.value().copy() if found else String()


struct LensSettings(Copyable, Movable):
    var model: String
    var model_label: String
    var models: List[SettingChoice]
    var effort: String
    var efforts: List[SettingChoice]

    def __init__(out self):
        self.model = String()
        self.model_label = String()
        self.models = List[SettingChoice]()
        self.effort = String()
        self.efforts = List[SettingChoice]()

    def effort_name(self) -> String:
        for choice in self.efforts:
            if choice.value == self.effort:
                return choice.name.copy()
        return self.effort.copy()

    def summary(self) -> String:
        var labelled = self.model_label.byte_length() > 0
        var effort = self.effort.byte_length() > 0
        if labelled and effort:
            return String(self.model_label, " · ", self.effort_name())
        if labelled:
            return self.model_label.copy()
        if effort:
            return self.effort_name()
        return String()

    def known(self, option: String) -> Bool:
        return option == SETTING_MODEL or option == SETTING_EFFORT

    def choices(self, option: String) -> List[SettingChoice]:
        if option == SETTING_MODEL:
            return self.models.copy()
        if option == SETTING_EFFORT:
            return self.efforts.copy()
        return List[SettingChoice]()

    def current(self, option: String) -> String:
        if option == SETTING_MODEL:
            return self.model.copy()
        if option == SETTING_EFFORT:
            return self.effort.copy()
        return String()


def settings_parse(json: Json, node: Int) -> Optional[LensSettings]:
    if not json.ok() or node < 0 or json.kind(node) != JSON_ARRAY:
        return None
    var options = json.children(node)
    var model = _find(json, options, SETTING_MODEL)
    if model < 0:
        return None
    var effort = _find(json, options, SETTING_EFFORT)
    var settings = LensSettings()
    settings.model = _current(json, model)
    var label = json.last_text(
        json.last(json.last(model, "_meta"), "zz"), "label"
    )
    if label and label.value().byte_length() > 0:
        settings.model_label = label.value().copy()
    else:
        settings.model_label = settings.model.copy()
    settings.models = _choices(json, model)
    if effort >= 0:
        settings.effort = _current(json, effort)
        settings.efforts = _choices(json, effort)
    return Optional[LensSettings](settings^)


def settings_from_json(text: String) -> Optional[LensSettings]:
    var json = json_text(text)
    return settings_parse(json, 0)


def _trim_slashes(text: String) -> String:
    var end = text.byte_length()
    while end > 0 and text.as_bytes()[end - 1] == 47:
        end -= 1
    return text_slice(text, 0, end)


def short_folder(folder: String, home: Optional[String]) -> String:
    var trimmed = _trim_slashes(folder)
    if home:
        var base = _trim_slashes(home.value())
        var width = base.byte_length()
        if width > 0 and trimmed.startswith(base):
            var rest = text_slice(trimmed, width, trimmed.byte_length())
            if rest.byte_length() == 0 or rest.as_bytes()[0] == 47:
                return String("~", rest)
    if trimmed.byte_length() == 0 and folder.byte_length() > 0:
        return "/"
    return trimmed^


def _has_space(text: String) -> Bool:
    for character in text.codepoints():
        if palette_space(UInt32(Int(character))):
            return True
    return False


def split_space(text: String) -> Optional[Setter]:
    var at = 0
    for character in text.codepoints():
        var scalar = UInt32(Int(character))
        var width = scalar_width(scalar)
        if palette_space(scalar):
            return Optional[Setter](
                Setter(
                    text_slice(text, 0, at),
                    Optional[String](
                        text_slice(text, at + width, text.byte_length())
                    ),
                )
            )
        at += width
    return None


def setter(text: String) -> Optional[Setter]:
    var trimmed = text_trim(text)
    if trimmed.byte_length() == 0 or trimmed.as_bytes()[0] != 47:
        return None
    var command = text_slice(trimmed, 1, trimmed.byte_length())
    var name = command.copy()
    var value = String()
    var parts = split_space(command)
    if parts:
        name = parts.value().option.copy()
        value = text_trim(parts.value().value.value())
    if name != SETTING_MODEL and name != SETTING_EFFORT:
        return None
    if _has_space(value):
        return None
    if value.byte_length() == 0:
        return Optional[Setter](Setter(name^, None))
    return Optional[Setter](Setter(name^, Optional[String](value^)))


def composer_setter(
    text: String, settings: Optional[LensSettings]
) -> Optional[ComposerSetter]:
    var found = setter(text)
    if not found:
        return None
    var option = found.value().option.copy()
    if not found.value().value:
        if settings and len(settings.value().choices(option)) > 0:
            return Optional[ComposerSetter](
                ComposerSetter(SETTER_LIST, option^, String())
            )
        return None
    var value = found.value().value.value().copy()
    if settings and settings.value().current(option) == value:
        return Optional[ComposerSetter](
            ComposerSetter(SETTER_KEEP, String(), String())
        )
    return Optional[ComposerSetter](ComposerSetter(SETTER_SET, option^, value^))


def source_message(item: String) -> Optional[String]:
    if not item.startswith("u:"):
        return None
    var end = item.byte_length()
    for index in range(2, end):
        if item.as_bytes()[index] == 126:
            end = index
            break
    var id = text_slice(item, 2, end)
    if id.byte_length() == 0 or id == "message" or id == "user":
        return None
    return Optional[String](id^)


def agent_program(provider: UInt8) -> String:
    return "claude" if provider == AGENT_CLAUDE else "codex"


def _command_line(words: List[String]) -> String:
    var line = String()
    for index in range(len(words)):
        if index > 0:
            line += " "
        line += palette_quote(words[index])
    return line^


def _forked_chat(
    opener: List[String],
    provider: UInt8,
    session: String,
    message: Optional[String],
    settings: Optional[LensSettings],
) -> List[String]:
    var select: List[String] = [
        "select-pane-kind",
        "--resume",
        session.copy(),
        "--fork",
    ]
    if message:
        select.append("--at")
        select.append(message.value().copy())
    if settings:
        if settings.value().model.byte_length() > 0:
            select.append("--model")
            select.append(settings.value().model.copy())
        if settings.value().effort.byte_length() > 0:
            select.append("--effort")
            select.append(settings.value().effort.copy())
    select.append("agent")
    select.append(agent_program(provider))
    return [
        "if-shell",
        "-F",
        "1",
        String(_command_line(opener), " ; ", _command_line(select)),
    ]


def fork_chat_command(
    provider: UInt8,
    session: String,
    cwd: String,
    message: Optional[String],
    settings: Optional[LensSettings],
) -> List[String]:
    var opener: List[String] = ["new-window", "-c", cwd.copy()]
    return _forked_chat(opener, provider, session, message, settings)


def side_chat_command(
    provider: UInt8,
    session: String,
    cwd: String,
    pane: UInt64,
    settings: Optional[LensSettings],
) -> List[String]:
    var opener: List[String] = [
        "new-pane",
        "-t",
        String("%", pane),
        "-c",
        cwd.copy(),
        "-x",
        SIDE_CHAT_WIDTH,
        "-y",
        SIDE_CHAT_HEIGHT,
        "-T",
        "Side chat",
    ]
    return _forked_chat(opener, provider, session, None, settings)


def real_session(session: String) -> Bool:
    return session.byte_length() > 0 and not session.startswith("lens-")
