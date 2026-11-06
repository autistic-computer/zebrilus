import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const artifacts = path.join(root, 'target', 'port');
const Reducer = (await import(pathToFileURL(path.join(artifacts, 'tree_reducer.mjs')))).default;
const count = 65535;
const none = {$: 'None'};
const empty = {$: 'Nil'};
const id = (high, low) => ({$: 'Id', high, low});
const tagged = (major, high, low, value) => ({$: 'Tagged', key: {$: 'SortKey', major, high, low}, value});

function list(values) {
  return values.reduceRight((tail, head) => ({$: 'Con', head, tail}), empty);
}

function values(items) {
  const result = [];
  while (items.$ === 'Con') {
    result.push(items.head);
    items = items.tail;
  }
  assert.equal(items.$, 'Nil');
  return result;
}

function session(index, windows = empty) {
  return {$: 'SessionRecord', name: index, active_window: id(0xffffffff, index), viewers: index, windows};
}

function window(index) {
  return {$: 'WindowRecord', name: index, active_pane: id(0xffffffff, index), layout: index,
    flags: index, presentation: index, panes: empty};
}

function sorted(nodes, major) {
  return nodes.slice().sort((a, b) => (major ? a.key.major - b.key.major : 0)
    || a.key.high - b.key.high || a.key.low - b.key.low);
}

const nodes = Array.from({length: count}, (_, index) => tagged(0, 0xfffffffe - index % 3,
  (count - index) % 257, session(index)));
const original = {$: 'TreeSnapshot', generation: id(0xffffffff, 17), focused: none, sessions: list(nodes)};
const removeAbsent = list([{$: 'RemoveSession', id: id(0xffffffff, 0xffffffff)}]);
const start = performance.now();
const result = Reducer.overlay(original, removeAbsent);
assert.equal(result.error.$, 'None');
assert.deepEqual(values(result.snapshot.sessions), sorted(nodes, false));
assert.deepEqual(result.snapshot.generation, original.generation);
assert.deepEqual(result.snapshot.focused, none);
assert.deepEqual(Reducer.overlay(original, empty).snapshot, original);

const windows = Array.from({length: count}, (_, index) => tagged((count - index) % 7,
  0xffffffff - index % 3, (count - index) % 257, window(index)));
const windowTree = {$: 'TreeSnapshot', generation: id(7, 11), focused: none,
  sessions: list([tagged(0, 0, 1, session(2, list(windows)))])};
const windowResult = Reducer.overlay(windowTree, removeAbsent);
assert.equal(windowResult.error.$, 'None');
assert.deepEqual(values(values(windowResult.snapshot.sessions)[0].value.windows), sorted(windows, true));

const report = {sessions: count, windows: count, seconds: (performance.now() - start) / 1000,
  result: 'large linked lists preserve exact lexicographic keys and full stable duplicate values'};
fs.writeFileSync(path.join(artifacts, 'tree-stress.json'), JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report));
