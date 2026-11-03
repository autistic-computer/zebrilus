# Introduction

penna is a widget kit for ala. The same widgets run natively, drawn on the GPU by ala, and in the browser as plain HTML and CSS. This site is built from the HTML version.

## Two builds, one look

Every widget has a Rust version and a DOM version. Both read the same theme tokens, so a button in the app and a button on a web page come out the same. A parity runner renders each storybook page both ways and scores how close they are.

## How the source is laid out

| Folder | What lives there |
| --- | --- |
| `foundation` | Theme, tokens, roles, motion and syntax highlighting |
| `components` | One widget each: button, input, kbd, switch, menu |
| `blocks` | Larger pieces made of components: settings, sidebar, title bar |

Each folder holds the Rust next to its CSS and JS, under the same name.

## Themes

penna ships three themes, `default`, `macos` and `yaru`, each in light and dark. Switch them with the picker in the corner of this page.
