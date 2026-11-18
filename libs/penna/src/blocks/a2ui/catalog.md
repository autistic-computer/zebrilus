Draw UI as one JSON object: `{"components":[...],"dataModel":{...}}`.

`components` is a flat list. Each has a unique `id` and a `component` name; containers name their children by id. One component must have the id `root`.

A prop marked `*` takes a literal or a binding `{"@path":"/json/pointer"}` into `dataModel`. Inputs write what the user enters back to the path their `value` is bound to. Inside a list template, a path with no leading `/` is read from the current item.

Layout
- `Column {children, justify?, align?}` stacks children. `Row {children, justify?, align?}` lays them side by side and wraps. `justify`: start, center, end, spaceBetween. `align`: start, center, end, stretch.
- `Grid {children, columns?}` equal columns, 3 by default.
- `Card {child}` a framed group around one child.
- `children` is a list of ids, or `{"componentId":"row","path":"/items"}` to repeat one component per item of a list.

Display
- `Text {text*, variant?, tone?}` variant: body, caption, heading, title, mono.
- `Stat {label*, value*}` a small label over a large value.
- `Badge {label*, tone?}`
- `Progress {value*, label?*}` value from 0 to 1.
- `Table {columns:[{title, path}], rows}` `rows` is the path of a list; each column's `path` is read from the row.
- `tone`: neutral, primary, danger, warning, success, accent.

Input
- `Button {label*, variant?, action}` variant: default, primary, ghost.
- `TextField {value*, label?*, placeholder?}`
- `Select {options, value*, label?*, placeholder?}` a dropdown.
- `Segmented {options, value*}` a few choices side by side.
- `Slider {value*, max, min?, step?, label?*}`
- `options` is a list of strings or of `{label, value}`.

Actions
- `action` is `{"event":{"name":"save","context":{"name":{"@path":"/form/name"}}}}`. On a click you receive the name and the context with its bindings resolved. Nothing else is sent to you.

Example

```json
{"components":[
 {"id":"root","component":"Column","children":["title","stats","name","save"]},
 {"id":"title","component":"Text","variant":"heading","text":"Deploy"},
 {"id":"stats","component":"Grid","columns":2,"children":["up","errors"]},
 {"id":"up","component":"Stat","label":"Uptime","value":{"@path":"/uptime"}},
 {"id":"errors","component":"Stat","label":"Errors","value":{"@path":"/errors"}},
 {"id":"name","component":"TextField","label":"Name","value":{"@path":"/form/name"}},
 {"id":"save","component":"Button","label":"Save","variant":"primary",
  "action":{"event":{"name":"save","context":{"name":{"@path":"/form/name"}}}}}
],"dataModel":{"uptime":"99.98%","errors":12,"form":{"name":"staging"}}}
```
