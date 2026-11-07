# WhichKey

`zzmojo.which_key.WhichKeyModel` ports the renderer-free row model. `rows(tables, table, prefix)` folds directional and numbered key families; passing `families=False` produces action rows. Group zero represents an absent group, and groups one through six follow the source enum order. `opens_all_keys` retains the exact one-command, one-argument condition.

The model validates the complete table vector against the shared prefix metadata limits before producing output. It uses the first matching table and first matching stock binding. The stock prefix table and exact implemented command names come from the current Rust source; `--update-data` explicitly refreshes the checked-in contract. General command canonicalization uses the existing full command catalog.

Custom binding detection compares all invocation fields retained by the wire format: raw argument bytes, source location, command blocks, alias expansion, optional stdin, and stdin availability. Notes affect labels independently of custom binding detection. Unicode whitespace, unsigned integer spelling, mouse-key substring filtering, family order, custom-set insertion order, repeat conjunction, and core disjunction follow the source.

Run `python3 scripts/port/guard.py python3 mojo/scripts/which_key_vectors.py` from zebrilus. The actual `zz_client::which_key::{rows,action_rows,opens_all_keys}` functions supply 6,729 reference cases, with 19,508 resulting rows, 20,840 key sets, 21,692 keys and 11,391 all-keys queries. The suite includes stock prefixes, all source tables, classification flags, aliases, prompt/confirmation wrappers, Unicode key shapes and whitespace, note overrides, metadata-only changes, feedback commands, duplicate table names, malformed command text, and family folding.

This check covers the bounded row model. Widget layout, native input ownership and complete application behavior require their separate checks. It does not establish a formal proof for the WhichKey renderer.
