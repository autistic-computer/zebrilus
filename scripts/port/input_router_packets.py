import json
import struct


MAGIC = 1230131266
OWNERS = ['NoOwner', 'Pane', 'Sidebar', 'Overlay', 'NativeEditor']
SURFACES = ['Terminal', 'Browser', 'Other']
EVENTS = ['ActivatePane', 'SetActivePane', 'FocusSidebar', 'FocusNativeEditor', 'OverlayOpened', 'OverlayClosed', 'NativeFocusObserved', 'PaneRemoved', 'Detached', 'WindowDeactivated']
KINDS = ['event', 'capture', 'suppress', 'key', 'drain', 'plain']


def identifier(value):
    return [value['high'], value['low']]


def token(value):
    return identifier(value['id'])


def optional(value, encode):
    return [0] if value['$'] == 'None' else [1, *encode(value['value'])]


def owner(value):
    tag = OWNERS.index(value['$'])
    return [tag, *identifier(value['pane']), SURFACES.index(value['surface']['$'])] if tag == 1 else [tag]


def active(value):
    return [*identifier(value['pane']), SURFACES.index(value['surface']['$'])]


def input_words(value):
    return [*token(value['key']), *optional(value['unshifted'], token), ['KeyPressed', 'Repeat', 'Release'].index(value['action']['$']), *token(value['payload'])]


def effect(value):
    if value['$'] == 'Forward':
        return [0, *identifier(value['pane']), *input_words(value['input'])]
    if value['$'] == 'Chrome':
        return [1, *token(value['action'])]
    return [2, *owner(value['owner'])]


def effects(values):
    return [len(values), *(word for value in values for word in effect(value))]


def state(value):
    result = [*identifier(value['keymap']), *owner(value['owner']), *optional(value['pending'], owner), *owner(value['observed']), *owner(value['restore']), *optional(value['active'], active), *optional(value['capture'], identifier)]
    held, local = value['claims']['held'], value['claims']['local']
    result.extend([len(held), *(word for value in held for word in [*token(value['key']), *identifier(value['pane'])])])
    result.extend([len(local), *(word for value in local for word in token(value))])
    return [*result, *effects(value['effects'])]


def operation(value):
    kind = value['kind']
    result = [KINDS.index(kind)]
    if kind == 'event':
        event = value['event']
        tag = EVENTS.index(event['$'])
        result.append(tag)
        if tag <= 1 or tag == 7:
            result.extend(identifier(event['pane']))
            if tag <= 1:
                result.append(SURFACES.index(event['surface']['$']))
        elif tag == 6:
            result.extend(owner(event['owner']))
    elif kind == 'capture':
        result.extend(optional(value['pane'], identifier))
    elif kind == 'suppress':
        result.extend(token(value['key']))
    elif kind == 'key':
        result.extend(input_words(value['input']))
        result.extend([int(value['prefix']['armed']), int(value['prefix']['claimed'])])
        for table in ['ui', 'sidebar', 'terminal', 'browser']:
            result.extend(optional(value['lookup'][table], token))
    elif kind == 'plain':
        result.extend(map(int, [value['releases'], value['ui_binds'], value['terminal_binds']]))
    return result


def request(value):
    return [MAGIC, 1, *state(value['before']), *operation(value)]


def outcome(value):
    result = [MAGIC, 1, *state(value['after'])]
    if value['kind'] == 'key':
        return [*result, 1, int(value['answer']['$'] == 'Native')]
    if value['kind'] == 'drain':
        return [*result, 2, *effects(value['answer'])]
    if value['kind'] == 'plain':
        return [*result, 3, int(value['answer'])]
    return [*result, 0]


def packed(words):
    return struct.pack('<' + str(len(words)) + 'I', *words)


def blob(value):
    padded = value + b'\0' * (-len(value) % 4)
    return [len(value), *struct.unpack('<' + str(len(padded) // 4) + 'I', padded)]


def payload_arena(reference):
    keys = reference['keys']
    by_key = {json.dumps(row['value'], sort_keys=True): row['id'] for row in keys}
    result = [len(keys), *(word for row in keys for word in identifier(row['id'])), len(reference['inputs'])]
    for row in reference['inputs']:
        raw = row['value']
        key = by_key[json.dumps(raw['key'], sort_keys=True)]
        unshifted = raw['unshifted_codepoint']
        result.extend(identifier(key))
        result.extend([0] if unshifted is None else [1, *identifier(by_key[json.dumps({'Character': unshifted}, sort_keys=True)])])
        result.extend([['Press', 'Repeat', 'Release'].index(raw['action']), *identifier(row['id'])])
        result.extend(blob(json.dumps(raw, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode()))
    return result


def trace_packet(trace, index, reference):
    actions = {tuple(identifier(value['action']['id'])) for value in trace['bindings']}
    bodies = {tuple(identifier(value['id'])): value['name'].encode() for value in reference['actions']}
    result = [len(actions)]
    for action in sorted(actions):
        result.extend([*action, *blob(bodies[action])])
    result.extend([*identifier(trace['initial']['keymap']), index + 1, len(trace['steps'])])
    for value in trace['steps']:
        before, expected = request(value), outcome(value)
        result.extend([len(before), *before, len(expected), *expected])
    return result
