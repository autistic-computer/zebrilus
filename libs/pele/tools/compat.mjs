import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';

const crate = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const args = process.argv.slice(2);
const flag = name => args.includes(name);
const argument = name => {
  const at = args.indexOf(name);
  if (at === -1) return undefined;
  if (!args[at + 1] || args[at + 1].startsWith('--')) throw new Error(`${name} requires a value`);
  return args[at + 1];
};

if (flag('--help')) {
  console.log('PELE_SOURCE=/path/to/pele PELE_NATIVE=/path/to/native/pele node tools/compat.mjs [--all-docs] [--output report.json] [--refresh-fixtures] [--refresh-text-fixtures] [--refresh-shape-fixtures]');
  process.exit(0);
}

try {
  const corpus = JSON.parse(fs.readFileSync(path.join(crate, 'tests/fixtures/compatibility.json'), 'utf8'));
  const status = JSON.parse(fs.readFileSync(path.join(crate, 'PORT_STATUS.json'), 'utf8'));
  const sourceDir = process.env.PELE_SOURCE;
  if (!sourceDir) throw new Error('Set PELE_SOURCE to a checkout of the pinned upstream Pele revision.');
  const revision = spawnSync('git', ['-C', sourceDir, 'rev-parse', 'HEAD'], { encoding: 'utf8' });
  if (revision.status !== 0) throw new Error(`Cannot read upstream checkout revision: ${revision.stderr.trim()}`);
  if (revision.stdout.trim() !== corpus.upstream.revision) throw new Error(`Upstream revision mismatch: expected ${corpus.upstream.revision}, found ${revision.stdout.trim()}`);
  const originalPath = path.join(sourceDir, 'dist/pele.js');
  if (!fs.existsSync(originalPath)) throw new Error(`Missing ${originalPath}; build upstream Pele first.`);
  const original = await import(pathToFileURL(originalPath));
  if (flag('--refresh-fixtures')) {
    const summarize = model => {
      switch (model.type) {
        case 'flowchart': return { direction: model.direction, nodes: [...model.nodes.values()].map(node => ({ id: node.id, label: node.text ?? node.id, shape: node.type ?? 'square' })), edges: model.edges.map(edge => ({ from: edge.start, to: edge.end, label: edge.text })), subgraphs: model.subgraphs.map(group => ({ id: group.id, title: group.title, nodes: group.nodes })) };
        case 'sequence': return { actors: [...model.actors].map(([id, actor]) => ({ id, label: actor.description, type: actor.type })), messages: model.messages.map(message => ({ from: message.from, to: message.to, text: message.message, type: message.type })), notes: model.notes };
        case 'pie': return { title: model.title ?? null, show_data: model.showData, sections: [...model.sections] };
        case 'quadrantChart': return { title: model.title ?? null, quadrants: model.quadrants, points: model.points.map(point => ({ label: point.text.text, x: point.x, y: point.y })) };
        default: return null;
      }
    };
    for (const fixture of corpus.fixtures) {
      try {
        const result = original.render(fixture.source, corpus.upstream.render_options);
        fixture.oracle = { ok: true, type: result.type, mermaid_type: result.mermaidType, width: result.width, height: result.height, model: summarize(original.parse(fixture.source)) };
      } catch (error) {
        fixture.oracle = { ok: false, error: { code: error.code, message: error.message, line: error.line, column: error.column } };
      }
    }
    fs.writeFileSync(path.join(crate, 'tests/fixtures/compatibility.json'), JSON.stringify(corpus, null, 2) + '\n');
  }
  if (flag('--refresh-text-fixtures')) {
    const { build } = await import(pathToFileURL(path.join(sourceDir, 'node_modules/esbuild/lib/main.js')));
    const contents = `export {layoutLabel} from ${JSON.stringify(path.join(sourceDir, 'src/text/label.ts'))}; export {metricsMeasurer} from ${JSON.stringify(path.join(sourceDir, 'src/text/measurer.ts'))};`;
    const bundled = await build({ stdin: { contents, resolveDir: sourceDir }, bundle: true, platform: 'node', format: 'esm', write: false });
    const helpers = await import('data:text/javascript;base64,' + Buffer.from(bundled.outputFiles[0].text).toString('base64'));
    const fixturePath = path.join(crate, 'tests/fixtures/text.json');
    const text = JSON.parse(fs.readFileSync(fixturePath, 'utf8'));
    for (const measurement of text.measurements) measurement.expected = helpers.metricsMeasurer.width(measurement.text, measurement.size, measurement.style);
    for (const item of text.labels) {
      const label = helpers.layoutLabel(item.raw, item.markdown, helpers.metricsMeasurer, item.size, item.max_width, item.base_style);
      item.expected = { lines: label.lines, widths: label.widths, width: label.width, height: label.height, size: label.size, line_height: label.lineHeight };
    }
    fs.writeFileSync(fixturePath, JSON.stringify(text, null, 2) + '\n');
  }
  if (flag('--refresh-shape-fixtures')) {
    const { build } = await import(pathToFileURL(path.join(sourceDir, 'node_modules/esbuild/lib/main.js')));
    const contents = `export {shapeSize, drawShape, shapeInset, shapeSpan, shapeHasLabel, shapeFilled} from ${JSON.stringify(path.join(sourceDir, 'src/svg/shapes.ts'))};`;
    const bundled = await build({ stdin: { contents, resolveDir: sourceDir }, bundle: true, platform: 'node', format: 'esm', write: false });
    const helpers = await import('data:text/javascript;base64,' + Buffer.from(bundled.outputFiles[0].text).toString('base64'));
    const fixturePath = path.join(crate, 'tests/fixtures/shape_geometry.json');
    const shapes = JSON.parse(fs.readFileSync(fixturePath, 'utf8'));
    if (shapes.upstreamRevision !== corpus.upstream.revision) throw new Error('Shape fixture upstream revision mismatch.');
    const { surface, border, line, radius } = shapes.palette;
    for (const item of shapes.cases) {
      const size = helpers.shapeSize(item.name, item.tw, item.th);
      const markup = helpers.drawShape(item.name, size.w, size.h, ' fill="var(--_s)" stroke="var(--_b)"', ' stroke="var(--_b)"');
      item.w = size.w;
      item.h = size.h;
      item.dy = size.dy;
      item.svg = markup.replaceAll(' style="rx:var(--_r)"', ` rx="${radius}"`).replaceAll('var(--_s)', surface).replaceAll('var(--_b)', border).replaceAll('var(--_l)', line);
      item.insets = [0, 1, 2, 3].map(side => helpers.shapeInset(item.name, size.w, size.h, side, shapes.insetOffset ?? 7));
      item.span = [false, true].map(sideways => helpers.shapeSpan(item.name, size.w, size.h, sideways));
      item.label = helpers.shapeHasLabel(item.name);
      item.filled = helpers.shapeFilled(item.name);
    }
    fs.writeFileSync(fixturePath, JSON.stringify(shapes, null, 2) + '\n');
  }
  if (flag('--refresh-fixtures') || flag('--refresh-text-fixtures') || flag('--refresh-shape-fixtures')) {
    console.log('Regenerated pinned upstream oracle fixtures.');
    process.exit(0);
  }
  const native = process.env.PELE_NATIVE ?? path.join(crate, 'target', 'debug', process.platform === 'win32' ? 'pele.exe' : 'pele');
  if (!fs.existsSync(native)) throw new Error(`Missing native binary ${native}; build the Rust crate first or set PELE_NATIVE.`);
  const supported = new Set(status.implemented.map(item => item.type));
  const cases = corpus.fixtures.map(fixture => ({ ...fixture, origin: 'pinned-fixture' }));
  if (flag('--all-docs')) {
    const docs = path.join(sourceDir, 'tests/corpus');
    for (const name of fs.readdirSync(docs).filter(name => name.endsWith('-docs.json')).sort()) {
      const diagrams = JSON.parse(fs.readFileSync(path.join(docs, name), 'utf8'));
      diagrams.forEach((source, index) => {
        const type = original.detectType(source);
        cases.push({ id: `${name}:${index}`, source, origin: 'upstream-docs', native_expectation: supported.has(type) ? 'render' : 'unsupported', labels: [] });
      });
    }
  }
  const requests = cases.map(item => JSON.stringify({ source: item.source, options: corpus.upstream.render_options })).join('\n') + '\n';
  const processResult = spawnSync(native, [], { input: requests, encoding: 'utf8', maxBuffer: 128 * 1024 * 1024, timeout: 120_000 });
  if (processResult.error) throw processResult.error;
  if (processResult.status !== 0) throw new Error(`Native renderer exited ${processResult.status}: ${processResult.stderr}`);
  const responses = processResult.stdout.trimEnd().split('\n').map((line, index) => {
    try { return JSON.parse(line); } catch { throw new Error(`Native response ${index + 1} is not JSON: ${line.slice(0, 120)}`); }
  });
  if (responses.length !== cases.length) throw new Error(`Native response count ${responses.length} differs from request count ${cases.length}.`);
  const results = cases.map((item, index) => {
    let reference;
    try {
      reference = { ok: true, ...original.render(item.source, corpus.upstream.render_options) };
    } catch (error) {
      reference = { ok: false, error: { code: error.code ?? 'internal', message: error.message, line: error.line, column: error.column } };
    }
    const actual = responses[index];
    const expected = item.origin === 'upstream-docs' && !reference.ok ? 'oracle-error' : item.native_expectation;
    const issues = [];
    if (expected === 'render') {
      if (!reference.ok) issues.push(`oracle rejected rendering target: ${reference.error.code}`);
      if (!actual.ok) issues.push(`native rejected rendering target: ${actual.error?.code}`);
      if (reference.ok && actual.ok && actual.type !== reference.type) issues.push(`diagram type differs: ${actual.type} vs ${reference.type}`);
    } else if (expected === 'unsupported') {
      if (actual.ok || actual.error?.code !== 'unsupported-diagram') issues.push('unported diagram did not return explicit unsupported-diagram');
    } else if (expected === 'error') {
      if (actual.ok || reference.ok || actual.error?.code !== reference.error?.code) issues.push('error category differs');
    } else if (expected === 'oracle-error') {
      if (actual.ok) issues.push('native accepted an upstream-invalid documentation example');
    } else {
      issues.push(`unknown native expectation ${expected}`);
    }
    let geometry;
    if (actual.ok) {
      if (!Number.isFinite(actual.width) || !Number.isFinite(actual.height) || actual.width <= 0 || actual.height <= 0) issues.push('non-positive or non-finite dimensions');
      if (typeof actual.svg !== 'string' || !actual.svg.startsWith('<svg')) issues.push('missing SVG document');
      if (/\s(?:style|fill|stroke|color|font(?:-family|-weight|-size|-style)?|rx|ry|x|y|cx|cy|r|width|height|transform|d)="[^"]*(?:var\(|color-mix\()[^"]*"|<(?:foreignObject|script)\b/.test(actual.svg ?? '')) issues.push('output is not static SVG');
      if (reference.ok) {
        geometry = {
          native: { width: actual.width, height: actual.height },
          upstream: { width: reference.width, height: reference.height },
          width_delta: actual.width - reference.width,
          height_delta: actual.height - reference.height,
          dimensions_match: Math.abs(actual.width - reference.width) < 0.011 && Math.abs(actual.height - reference.height) < 0.011,
          svg_bytes_equal: actual.svg === reference.svg,
        };
      }
    }
    return { id: item.id, origin: item.origin, native_expectation: expected, upstream_ok: reference.ok, native_ok: actual.ok, type: reference.type ?? actual.type ?? original.detectType(item.source), pass: issues.length === 0, issues, geometry, native_error: actual.ok ? undefined : actual.error, upstream_error: reference.ok ? undefined : reference.error };
  });
  const summary = {
    fixtures: results.length,
    passed_expectations: results.filter(result => result.pass).length,
    failed_expectations: results.filter(result => !result.pass).length,
    rendering_targets: results.filter(result => result.native_expectation === 'render').length,
    native_rendered: results.filter(result => result.native_ok).length,
    intentionally_unported: results.filter(result => result.native_expectation === 'unsupported').length,
    expected_errors: results.filter(result => result.native_expectation === 'error').length,
    upstream_invalid_docs: results.filter(result => result.native_expectation === 'oracle-error').length,
    matching_dimensions: results.filter(result => result.geometry?.dimensions_match).length,
    rendered_types: [...new Set(results.filter(result => result.native_ok).map(result => result.type))].sort(),
    exact_svg_or_pixel_parity_asserted: false,
  };
  const report = { upstream_revision: corpus.upstream.revision, native_binary: native, note: corpus.comparison, summary, results };
  const output = argument('--output');
  if (output) fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(summary, null, 2));
  for (const result of results.filter(result => !result.pass)) console.error(`${result.id}: ${result.issues.join('; ')}`);
  process.exitCode = summary.failed_expectations > 0 ? 1 : 0;
} catch (error) {
  console.error(error.message);
  process.exitCode = 2;
}
