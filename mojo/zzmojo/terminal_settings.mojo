from .appearance_protocol import TerminalAppearance, FontFeature
from .client_events import _ascii_lower


def _available(family: String, names: List[String]) raises -> Bool:
    var folded = _ascii_lower(family)
    for name in names:
        if _ascii_lower(name) == folded:
            return True
    return False


def _localize(
    families: List[String], source: UInt8, available: List[String]
) raises -> List[String]:
    var result = List[String]()
    if source != 0:
        for family in families:
            if _available(family, available):
                result.append(family)
    return result^


def localized_appearance(
    appearance: TerminalAppearance,
    provenance: Dict[UInt8, UInt8],
    available: List[String],
    fallback: String,
    preferred: Optional[String] = None,
    scale: Optional[Float32] = None,
) raises -> TerminalAppearance:
    var result = appearance.copy()
    result.font_families = _localize(
        appearance.font_families, provenance.get(7, 0), available
    )
    result.font_families_bold = _localize(
        appearance.font_families_bold, provenance.get(8, 0), available
    )
    result.font_families_italic = _localize(
        appearance.font_families_italic, provenance.get(9, 0), available
    )
    result.font_families_bold_italic = _localize(
        appearance.font_families_bold_italic, provenance.get(10, 0), available
    )
    if len(result.font_families) == 0:
        result.font_families.append(fallback)
    if Bool(preferred) and _available(preferred.value(), available):
        result.font_families.clear()
        result.font_families.append(preferred.value())
        result.font_families_bold.clear()
        result.font_families_italic.clear()
        result.font_families_bold_italic.clear()
    if scale:
        result.font_size_points = (
            result.font_size_points * scale.value()
        ).clamp(7, 48)
    return result^


@fieldwise_init
struct FontRequest(Copyable, Movable):
    var family: String
    var fallbacks: List[String]
    var features: List[FontFeature]
    var weight: UInt16
    var italic: Bool


def _request(
    appearance: TerminalAppearance,
    fallback: String,
    configured: List[String],
    bold: Bool,
    italic: Bool,
) -> FontRequest:
    var families = (
        configured.copy() if len(configured)
        > 0 else appearance.font_families.copy()
    )
    var family = fallback if len(families) == 0 else families[0]
    var fallbacks = List[String]()
    for index in range(1, len(families)):
        fallbacks.append(families[index])
    var features: List[FontFeature] = [FontFeature(0x6167696C, 1)]
    features.extend(Span(appearance.font_features))
    var weight = UInt16(
        min(Int(appearance.font_weight) + (300 if bold else 0), 900)
    )
    return FontRequest(family^, fallbacks^, features^, weight, italic)


def font_request(
    appearance: TerminalAppearance,
    fallback: String,
    bold: Bool = False,
    italic: Bool = False,
) -> FontRequest:
    if bold and italic:
        return _request(
            appearance,
            fallback,
            appearance.font_families_bold_italic,
            bold,
            italic,
        )
    if bold:
        return _request(
            appearance, fallback, appearance.font_families_bold, bold, italic
        )
    if italic:
        return _request(
            appearance, fallback, appearance.font_families_italic, bold, italic
        )
    return _request(
        appearance, fallback, appearance.font_families, bold, italic
    )


def display_size(appearance: TerminalAppearance, delta: Float32 = 0) -> Float32:
    return (appearance.font_size_points + delta).clamp(7, 48)
