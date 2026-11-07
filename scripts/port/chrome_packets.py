

def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def blob(output, value):
    uint(output, len(value))
    output.extend(value)


def text(output, value):
    blob(output, value.encode())


def action(output, value):
    uint(output, value + 1)


def parsed(output, value):
    uint(output, value is not None)
    if value is not None:
        uint(output, value['modifiers'])
        text(output, value['base'])
        text(output, value['display'])


def snapshot(output, value):
    uint(output, len(value['bindings']))
    for table, key, code in value['bindings']:
        text(output, table)
        text(output, key)
        uint(output, code)
    uint(output, len(value['tables']))
    for table in value['tables']:
        text(output, table['name'])
        uint(output, table['present'])
        uint(output, len(table['bindings']))
        for modifiers, base, display, code in table['bindings']:
            uint(output, modifiers)
            text(output, base)
            text(output, display)
            uint(output, code)


def write_packets(reference, directory):
    output = bytearray()
    for name in ['actions', 'names', 'parsed', 'normalized']:
        uint(output, len(reference[name]))
        for row in reference[name]:
            if name == 'actions':
                uint(output, row['code'])
                text(output, row['name'])
                action(output, row['parsed'])
            elif name == 'names':
                text(output, row['name'])
                action(output, row['action'])
            elif name == 'parsed':
                text(output, row['spelling'])
                parsed(output, row['value'])
            else:
                uint(output, row['flags'])
                text(output, row['base'])
                uint(output, row['modifiers'])
                text(output, row['normalized'])
                text(output, row['display'])
    (directory / 'direct.bin').write_bytes(output)
    output = bytearray()
    uint(output, len(reference['inputs']))
    for row in reference['inputs']:
        blob(output, row['wire'])
    (directory / 'inputs.bin').write_bytes(output)
    output = bytearray()
    uint(output, len(reference['cases']))
    for index, case in enumerate(reference['cases']):
        uint(output, reference['profiles'][index])
        uint(output, 100 + index)
        snapshot(output, case['initial'])
        uint(output, len(case['steps']))
        for step in case['steps']:
            values = step['request']
            uint(output, values[0])
            text(output, values[1])
            if values[0] == 3:
                uint(output, values[2])
            else:
                text(output, values[2])
                if values[0] == 0:
                    text(output, values[3])
            if values[0] == 0:
                uint(output, step['answer'] is not None)
                if step['answer'] is not None:
                    text(output, step['answer'])
            elif values[0] == 1:
                uint(output, step['answer'])
            else:
                action(output, step['answer'])
            uint(output, step['changed'])
            snapshot(output, step['after'])
        snapshot(output, case['after'])
        uint(output, len(case['resolved']))
        for row in case['resolved']:
            text(output, row['table'])
            uint(output, len(row['actions']))
            for value in row['actions']:
                action(output, value)
    (directory / 'maps.bin').write_bytes(output)
