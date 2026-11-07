import hashlib


def write_catalog(reference, root):
    names = {}
    for row in reference['actions']:
        kind, value = divmod(row['code'], 256)
        if value == 0 or value == 255:
            names[kind] = row['name']
    assert sorted(names) == list(range(len(names)))
    rows = []
    for profile, bindings in enumerate(reference['defaults']):
        for table, key, action in bindings:
            assert '\t' not in table + key and '\n' not in table + key
            rows.append(f'{profile}\t{table}\t{key}\t{action}')
    source = root / 'crates/zz-client/src/chrome.rs'
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    value = f'comptime CHROME_CATALOG_SOURCE = "{digest}"\n\n'
    value += 'comptime CHROME_ACTION_NAMES = """' + '\n'.join(names[index] for index in sorted(names)) + '"""\n\n'
    value += 'comptime CHROME_PROFILE_DEFAULTS = """' + '\n'.join(rows) + '"""\n'
    destination = root / 'mojo/zzmojo/chrome_catalog.mojo'
    if not destination.is_file() or destination.read_text() != value:
        destination.write_text(value)
