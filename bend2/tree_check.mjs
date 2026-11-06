import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const artifacts = path.join(root, 'target', 'port');
const Reducer = (await import(pathToFileURL(path.join(artifacts, 'tree_reducer.mjs')))).default;
const oracle = JSON.parse(fs.readFileSync(path.join(artifacts, 'tree-reducer-oracle.json'), 'utf8'));
assert.equal(oracle.reference, 'zz_protocol::TreeDelta::apply/apply_overlay + ClientView::apply');

function bend(value) {
  if (Array.isArray(value)) {
    return value.reduceRight((tail, head) => ({$: 'Con', head: bend(head), tail}), {$: 'Nil'});
  }
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, field]) => [key, bend(field)]));
  }
  return value;
}

function plain(value) {
  if (value && typeof value === 'object') {
    if (value.$ === 'Con' || value.$ === 'Nil') {
      const output = [];
      while (value.$ === 'Con') {
        output.push(plain(value.head));
        value = value.tail;
      }
      assert.equal(value.$, 'Nil');
      return output;
    }
    return Object.fromEntries(Object.entries(value).map(([key, field]) => [key, plain(field)]));
  }
  return value;
}

function integer(value) {
  if (typeof value === 'object') return (BigInt(value.high) << 32n) | BigInt(value.low);
  return BigInt(value);
}

function varint(value) {
  value = integer(value);
  const bytes = [];
  while (value >= 128n) {
    bytes.push(Number(value & 127n) | 128);
    value >>= 7n;
  }
  bytes.push(Number(value));
  return Buffer.from(bytes);
}

function maybeId(value) {
  return value.$ === 'None' ? Buffer.from([0]) : Buffer.concat([Buffer.from([1]), varint(value.value)]);
}

function nodeId(node) {
  return {high: node.key.high, low: node.key.low};
}

function wire(snapshot, arena) {
  function group(index, expected) {
    const value = arena[index];
    assert.ok(value);
    assert.equal(value.group, expected);
    assert.deepEqual(Buffer.from(value.bytes), Buffer.concat(value.parts.map(part => Buffer.from(part))));
    return value.parts.map(part => Buffer.from(part));
  }

  function pane(entry) {
    const value = entry.value;
    return Buffer.concat([varint(nodeId(entry)), varint(value.id), ...group(value.title, 'pane-title'),
      ...group(value.state, 'pane-state'), ...group(value.presentation, 'pane-presentation'),
      ...group(value.status, 'pane-status'), ...group(value.summary, 'pane-summary')]);
  }

  function window(entry) {
    const value = entry.value;
    const layout = group(value.layout, 'window-layout');
    const flags = group(value.flags, 'window-flags');
    const presentation = group(value.presentation, 'window-presentation');
    return Buffer.concat([varint(nodeId(entry)), varint(entry.key.major), ...group(value.name, 'window-name'),
      flags[0], varint(value.active_pane), layout[1], layout[0], varint(value.panes.length),
      ...value.panes.map(pane), layout[2], layout[3], presentation[0], flags[1], flags[2],
      presentation[1], presentation[2], presentation[3], ...layout.slice(4), flags[3]]);
  }

  function session(entry) {
    const value = entry.value;
    return Buffer.concat([varint(nodeId(entry)), ...group(value.name, 'session-name'),
      varint(value.active_window), varint(value.windows.length), ...value.windows.map(window),
      ...group(value.viewers, 'session-viewers')]);
  }

  return Buffer.concat([varint(snapshot.generation), varint(snapshot.sessions.length),
    ...snapshot.sessions.map(session), maybeId(snapshot.focused)]);
}

let accepted = 0;
let bytes = 0;
for (const vector of oracle.vectors) {
  assert.deepEqual(wire(vector.before, vector.arena), Buffer.from(vector.before_wire), vector.name + ' input full snapshot');
  const snapshot = bend(vector.before);
  const update = bend(vector.update);
  const result = vector.kind === 'delta' ? Reducer.apply(snapshot, update)
    : vector.kind === 'view' ? Reducer.client_view(snapshot, update) : Reducer.overlay(snapshot, update);
  const actual = plain(result);
  assert.deepEqual(actual, vector.outcome, vector.name + ' complete normalized graph');
  assert.deepEqual(wire(actual.snapshot, vector.arena), Buffer.from(vector.after_wire), vector.name + ' output full snapshot');
  accepted += Number(actual.error.$ === 'None');
  bytes += vector.after_wire.length;
}
const report = {reference: oracle.reference, cases: oracle.vectors.length, accepted,
  rejected: oracle.vectors.length - accepted, complete_snapshot_bytes_compared: bytes,
  result: 'all complete graph and resolved snapshot byte comparisons passed'};
fs.writeFileSync(path.join(artifacts, 'tree-reducer-differential.json'), JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report));
