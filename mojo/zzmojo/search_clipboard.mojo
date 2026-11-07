from mjui.input import Clipboard, TextInput, TEXT_COMMAND, CMD_PASTE


struct SearchClipboard(Copyable, Movable, Clipboard):
    var text: String
    var primary_text: String
    var primary_supported: Bool
    var clipboard_loaded: Bool
    var pending_clipboard: Optional[String]
    var pending_primary: Optional[String]

    def __init__(out self):
        self.text = String()
        self.primary_text = String()
        self.primary_supported = False
        self.clipboard_loaded = False
        self.pending_clipboard = None
        self.pending_primary = None

    def get_clipboard(self) -> String:
        return self.text

    def get_clipboard_bounded(self, maximum: Int) raises -> String:
        if maximum < 0 or self.text.byte_length() > maximum:
            raise Error("search clipboard exceeds byte limit")
        return self.text

    def set_clipboard(mut self, text: String):
        self.text = text.copy()
        self.clipboard_loaded = True
        self.pending_clipboard = text

    def load[Host: Clipboard](mut self, host: Host, maximum: Int) raises:
        if not self.clipboard_loaded:
            self.text = host.get_clipboard_bounded(maximum)
            self.clipboard_loaded = True

    def supports_primary(self) -> Bool:
        return self.primary_supported

    def get_primary_selection(self) -> String:
        return self.primary_text

    def get_primary_selection_bounded(self, maximum: Int) raises -> String:
        if maximum < 0 or self.primary_text.byte_length() > maximum:
            raise Error("search primary selection exceeds byte limit")
        return self.primary_text

    def set_primary_selection(mut self, text: String):
        self.primary_text = text.copy()
        self.pending_primary = text

    def deliver[Host: Clipboard](mut self, mut host: Host):
        if self.pending_clipboard:
            host.set_clipboard(self.pending_clipboard.value())
        if self.pending_primary:
            host.set_primary_selection(self.pending_primary.value())
        self.pending_clipboard = None
        self.pending_primary = None


def capture_search_clipboard[Host: Clipboard](host: Host, input: TextInput, maximum: Int, paste_enabled: Bool, primary_enabled: Bool) raises -> SearchClipboard:
    if maximum < 0:
        raise Error("search clipboard bound is negative")
    var result = SearchClipboard()
    result.primary_supported = host.supports_primary()
    if paste_enabled:
        for event in input.events:
            if event.kind == TEXT_COMMAND and event.cmd == CMD_PASTE:
                result.load(host, maximum)
                break
    if primary_enabled and input.primary and result.primary_supported:
        result.primary_text = host.get_primary_selection_bounded(maximum)
    return result^
