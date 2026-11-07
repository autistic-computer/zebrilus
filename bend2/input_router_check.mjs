import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const artifacts = path.join(root, 'target', 'port');
const Router = (await import(pathToFileURL(path.join(artifacts, 'input_router.mjs')))).default;
const Data = (await import(pathToFileURL(path.join(artifacts, 'input_router_data.mjs')))).default;
const source = path.join(artifacts, 'input-router-oracle.json');
assert(fs.statSync(source).size <= 64 * 1024 * 1024, 'source trace exceeds 64 MiB');
const oracle = JSON.parse(fs.readFileSync(source, 'utf8'));
assert.equal(oracle.reference, 'zz_client::InputRouter + actual ChromeKeymap');
assert(oracle.traces.length <= 256);

function bend(value) {
  if (Array.isArray(value)) return value.reduceRight((tail, head) => ({$: 'Con', head: bend(head), tail}), {$: 'Nil'});
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, field]) => [key, bend(field)]));
  return value;
}

function plain(value) {
  if (value && typeof value === 'object') {
    if (value.$ === 'Nil' || value.$ === 'Con') {
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

let steps = 0;
let forwarded = 0;
let chrome = 0;
let heldPeak = 0;
let localPeak = 0;
let effectPeak = 0;
const commands = new Map();
for (const trace of oracle.traces) {
  assert(trace.steps.length <= 512);
  let state = bend(trace.initial);
  assert.deepEqual(plain(Data.new(bend(trace.initial.keymap))), trace.initial, trace.name);
  for (const [index, command] of trace.steps.entries()) {
    const context = `${trace.name}:${index}:${command.kind}`;
    assert.deepEqual(plain(state), command.before, `${context}: before`);
    let answer = null;
    switch (command.kind) {
      case 'event': state = Router.event(state, bend(command.event)); break;
      case 'capture': state = Router.set_capture(state, bend(command.pane)); break;
      case 'suppress': state = Router.suppress_release(state, bend(command.key)); break;
      case 'key': {
        const result = Router.key(state, bend(command.input), bend(command.prefix), bend(command.lookup));
        state = result.state;
        answer = plain(result.disposition);
        break;
      }
      case 'drain': {
        const result = Router.drain_effects(state);
        state = result.state;
        answer = plain(result.effects);
        break;
      }
      case 'plain': answer = Router.passes_plain_keys(state, command.releases, command.ui_binds, command.terminal_binds); break;
      default: assert.fail(`unknown source operation ${command.kind}`);
    }
    const after = plain(state);
    assert.deepEqual(after, command.after, `${context}: complete state`);
    assert.deepEqual(answer, command.answer, `${context}: result`);
    assert.deepEqual(plain(Data.owner(state)), command.effective_owner, `${context}: effective owner`);
    heldPeak = Math.max(heldPeak, after.claims.held.length);
    localPeak = Math.max(localPeak, after.claims.local.length);
    effectPeak = Math.max(effectPeak, after.effects.length);
    forwarded += after.effects.slice(command.before.effects.length).filter(effect => effect.$ === 'Forward').length;
    chrome += after.effects.slice(command.before.effects.length).filter(effect => effect.$ === 'Chrome').length;
    commands.set(command.kind, (commands.get(command.kind) || 0) + 1);
    steps++;
  }
}
console.log(JSON.stringify({passed: true, traces: oracle.traces.length, steps, forwarded, chrome, held_peak: heldPeak, local_peak: localPeak, effects_peak: effectPeak, operations: Object.fromEntries(commands), scope: 'actual source router complete ordered state/effects and eligibility, with actual keymap resolutions and typed opaque key/action/payload references'}));
