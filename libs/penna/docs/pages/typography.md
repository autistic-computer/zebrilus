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

A page title says where you are, a heading names a group, body text does the work, and small text explains. Two or three of these in one view is plenty. More steps make a screen harder to scan, not easier.

```figure uses=label,kbd
<div class="docs-full" style="display: flex; flex-direction: column; gap: 32px; max-width: 460px">
  <div style="display: flex; flex-direction: column; gap: 4px">
    <span class="penna-type" data-type="title" data-penna-text>Keyboard</span>
    <span class="penna-type docs-text" data-type="body" data-muted data-penna-text>Shortcuts work in every window, including the storybook.</span>
  </div>
  <div style="display: flex; flex-direction: column; gap: 8px">
    <span class="penna-type" data-type="heading" data-penna-text>Panes</span>
    <div data-penna="settings.card" style="display: flex; flex-direction: column; padding: 4px 12px">
      <div class="docs-row" style="padding: 8px 0"><span class="penna-type" data-type="body" data-penna-text>Next pane</span><span class="docs-spacer"></span><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="right"></kbd></div>
      <div class="docs-row" style="padding: 8px 0"><span class="penna-type" data-type="body" data-penna-text>Previous pane</span><span class="docs-spacer"></span><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="left"></kbd></div>
      <div class="docs-row" style="padding: 8px 0"><span class="penna-type" data-type="body" data-penna-text>Command palette</span><span class="docs-spacer"></span><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd></div>
    </div>
    <span class="penna-type docs-text" data-type="caption" data-muted data-penna-text>Change a shortcut by clicking it and pressing new keys.</span>
  </div>
</div>
```

## Emphasis

Weight does the emphasis, not size. Medium marks the row that is picked or a label that matters; semibold is for headings. A word that needs to stand out gets a heavier weight, never a bigger size or a color.

## Reading

Documents and chat use a larger, more open size than the interface, with headings that step up clearly from the text.

```figure story=markdown/headings
```

## Code

Code takes the size of the text around it, in Lilex, so a command inside a sentence stays on the line.

```figure story=markdown/code
```
