Draw UI as a few lines of calls. `root` is the entry point.

```
root = Column([title, Row([name, save])])
title = Text("Deploy", "heading")
name = TextField($/form/name, "Name")
save = Button("Save", Event("save", {name: $/form/name}), "primary")
$/form/name = "staging"
```

Rules
- A line is `name = Component(...)`. Components can also be written inline inside another call. Arguments go in the order shown below, or by name: `Text("Hi", tone="danger")`.
- Strings use double quotes. Lists are `[a, b]`, maps are `{key: value}`, plus numbers, true and false.
- `$/path` binds a value to the data model. Inputs write what the user enters back to the path their `value` is bound to. `$/path = value` on its own line sets a starting value (a string, number, list or map).
- To repeat a component per item of a list: `Column(_template($/items, row))`. Inside `row`, `$field` is read from the current item.
- An action is `Event("name", {key: $/path})`. On a click you receive the name and the map with its bindings resolved. Nothing else is sent to you.

Layout
- `Column(children, justify?, align?)` stacks children. `Row(children, justify?, align?)` lays them side by side and wraps. justify: start, center, end, spaceBetween. align: start, center, end, stretch.
- `Grid(children, columns?)` equal columns, 3 by default.
- `Card(child)` a framed group around one child.

Display
- `Text(text, variant?, tone?)` variant: body, caption, heading, title, mono.
- `Stat(label, value)` a small label over a large value.
- `Badge(label, tone?)`
- `Progress(value, label?)` value from 0 to 1.
- `Table(columns, rows)` columns is `[{title: "Name", path: "name"}]`, rows is the path of a list such as `$/services`.
- `Chart(rows, x, series, kind?)` rows is the path of a list, x the field with each item's label, series the number fields to plot, at most 4: `Chart($/days, "day", [{title: "Errors", path: "errors"}], "line")`. kind: bar, line.
- tone: neutral, primary, danger, warning, success, accent.

Input
- `Button(label, action, variant?)` variant: default, primary, ghost.
- `TextField(value, label?, placeholder?)`
- `Select(options, value, label?, placeholder?)` a dropdown.
- `Segmented(options, value)` a few choices side by side.
- `Slider(value, max, min?, step?, label?)`
- options is a list of strings or of `{label: "A", value: "a"}`.
