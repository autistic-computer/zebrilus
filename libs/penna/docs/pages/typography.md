# Typography

penna sets the interface in one typeface and code in another. Most of what you read in a window is one size. A handful of larger steps and a heavier weight carry the hierarchy, so screens stay even and easy to scan.

```figure uses=label
<div class="docs-ladder docs-full">
  <span class="penna-type" data-type="small" data-penna-text>Display</span><span class="penna-type" data-type="display" data-penna-text>No sessions yet</span>
  <span class="penna-type" data-type="small" data-penna-text>Title</span><span class="penna-type" data-type="title" data-penna-text>Keyboard shortcuts</span>
  <span class="penna-type" data-type="small" data-penna-text>Heading</span><span class="penna-type" data-type="heading" data-penna-text>Appearance</span>
  <span class="penna-type" data-type="small" data-penna-text>Lead</span><span class="penna-type" data-type="lead" data-penna-text>Pick a theme for every window</span>
  <span class="penna-type" data-type="small" data-penna-text>Body</span><span class="penna-type" data-type="body" data-penna-text>Split the pane to the right</span>
  <span class="penna-type" data-type="small" data-penna-text>Small</span><span class="penna-type" data-type="small" data-penna-text>Show hidden files</span>
  <span class="penna-type" data-type="small" data-penna-text>Caption</span><span class="penna-type" data-type="caption" data-penna-text>Used by three windows</span>
  <span class="penna-type" data-type="small" data-penna-text>Micro</span><span class="penna-type" data-type="micro" data-penna-text>12 new</span>
  <span class="penna-type" data-type="small" data-penna-text>Code</span><span class="penna-type" data-type="mono" data-penna-text>git log --stat</span>
</div>
```

## Hierarchy

A page title says where you are, a bolder label names a group, body text does the work, and small text explains. Two or three of these in one view is plenty. More steps make a screen harder to scan, not easier.

```figure uses=label,kbd
<div class="docs-full penna-stack" data-gap="section" style="max-width: 460px">
  <div class="penna-stack" data-gap="tight">
    <span class="penna-type" data-type="title" data-penna-text>Keyboard</span>
    <span class="penna-type docs-text" data-type="body" data-muted data-penna-text>Shortcuts work in every window, including the storybook.</span>
  </div>
  <div class="penna-settings" style="padding: 0">
    <div>
      <h2>Panes</h2>
      <div data-penna="settings.card">
        <div class="penna-setting"><div class="penna-copy">Next pane<small>Move to the pane on the right.</small></div><span class="penna-inline" data-gap="tight"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="right"></kbd></span></div>
        <div class="penna-setting"><div class="penna-copy">Previous pane<small>Move to the pane on the left.</small></div><span class="penna-inline" data-gap="tight"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="left"></kbd></span></div>
        <div class="penna-setting"><div class="penna-copy">Command palette<small>Click any shortcut to change it.</small></div><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd></div>
      </div>
    </div>
  </div>
</div>
```

## Emphasis

Weight does the emphasis, not size. Medium marks the row that is picked or a label that matters; semibold is for titles, headings and the labels that name a group. A word that needs to stand out gets a heavier weight, never a bigger size or a color.

## Reading

Documents and chat use a larger, more open size than the interface, with headings that step up clearly from the text.

```figure story=markdown/headings
```

## Touch

On a phone and in sheets, text steps up: a small size for details, a body size for rows and labels, and a large title. It is made to read at arm's length and to sit in rows a finger can hit.

```figure uses=label
<div class="docs-ladder docs-full">
  <span class="penna-type" data-type="small" data-penna-text>Title</span><span class="penna-type" data-type="touch-title" data-penna-text>Language</span>
  <span class="penna-type" data-type="small" data-penna-text>Body</span><span class="penna-type" data-type="touch-body" data-penna-text>Split right</span>
  <span class="penna-type" data-type="small" data-penna-text>Small</span><span class="penna-type" data-type="touch-small" data-penna-text>Two panes, one window</span>
</div>
```

## Code

Code takes the size of the text around it, in the platform's code typeface, so a command inside a sentence stays on the line. This site sets code in Lilex.

```figure story=markdown/code
```
