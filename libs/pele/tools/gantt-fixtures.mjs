import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

process.env.TZ = 'UTC';
if (new Date(2024, 6, 1).getTimezoneOffset() !== 0) throw new Error('Could not switch the oracle to UTC.');

const revision = '36e584d59152181927e0d8bf70fddcd2a5f33d6c';
const crate = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const source = process.env.PELE_SOURCE;
if (!source) throw new Error('Set PELE_SOURCE to the pinned Pele checkout with npm dependencies installed and dist built.');
const actual = execFileSync('git', ['-C', source, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
if (actual !== revision) throw new Error(`Expected Pele ${revision}, found ${actual}.`);

const pele = await import(pathToFileURL(path.join(source, 'dist/pele.js')).href);
const { build } = await import(pathToFileURL(path.join(source, 'node_modules/esbuild/lib/main.js')).href);
const contents = [
  `export {parseStrict, format, add} from ${JSON.stringify(path.join(source, 'src/diagrams/gantt/dates.ts'))};`,
  `export {autoTicks, intervalTicks, timeFormat} from ${JSON.stringify(path.join(source, 'src/diagrams/gantt/axis.ts'))};`,
  `export {parseDuration} from ${JSON.stringify(path.join(source, 'src/diagrams/gantt/db.ts'))};`,
].join('\n');
const bundled = await build({ stdin: { contents, resolveDir: source }, bundle: true, platform: 'node', format: 'esm', write: false });
const dates = await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`);

const NOW = 0;
const palette = {
  '--_bg': '#ffffff',
  '--_fg': '#1f1f1f',
  '--_m': '#6e6e6e',
  '--_l': '#1f1f1f',
  '--_s': '#f3f3f3',
  '--_a': '#e6e6e6',
  '--_b': '#8a8a8a',
  '--_c': '#5b7bd5',
  critical: '#e45756',
  radius: 4,
};
const base = { fontSize: 16, fontFamily: 'Arial', padding: 8, responsive: false };

const body = (svg) =>
  svg
    .replace(/^<svg[^>]*>/, '')
    .replace(/<\/svg>$/, '')
    .replace(/^<title[^>]*>[\s\S]*?<\/title>/, '')
    .replace(/^<desc[^>]*>[\s\S]*?<\/desc>/, '')
    .replace(/<g class="pele-today">.*?<\/g>/, '')
    .replaceAll('font-weight="var(--_tw)"', 'font-weight="bold"')
    .replaceAll(' style="rx:var(--_r)"', ` rx="${palette.radius}"`)
    .replaceAll('var(--pele-critical,#e45756)', palette.critical)
    .replace(/var\((--_[a-z]+)\)/g, (_, name) => {
      if (!(name in palette)) throw new Error(`No static paint for ${name}`);
      return palette[name];
    })
    .replace(/(<text[^>]*?) font-size="[^"]*"/g, '$1');

const h = 'gantt\ndateFormat YYYY-MM-DD\n';
const docs = JSON.parse(fs.readFileSync(path.join(source, 'tests/corpus/gantt-docs.json'), 'utf8'));
const crafted = [
  ['axis-directives', h + 'title Formats\naxisFormat %a %b %e %j %U %W %V %G %g %u %w %y\nA :2024-01-01, 40d\n'],
  ['axis-locale', h + 'axisFormat %c\nA :2024-12-28, 10d\n'],
  ['axis-misc', h + 'axisFormat %x %X %p %I %L %f %Q %s %q %Z %%\nA :2024-03-01, 2d\n'],
  ['axis-padding', h + 'axisFormat %-d/%_m/%0Y %k\nA :2024-03-01, 9d\n'],
  ['tick-week-monday', h + 'tickInterval 1week\nweekday monday\nA :2024-01-01, 60d\n'],
  ['tick-two-weeks-thursday', h + 'tickInterval 2week\nweekday thursday\nA :2024-01-01, 90d\n'],
  ['tick-ten-days', h + 'tickInterval 10day\nA :2024-01-01, 60d\n'],
  ['tick-month', h + 'tickInterval 1month\nA :2024-01-31, 200d\n'],
  ['tick-quarter', h + 'tickInterval 3month\nA :2023-11-30, 700d\n'],
  ['tick-hours', h + 'tickInterval 6hour\nA :2024-01-01, 3d\n'],
  ['tick-minutes', h + 'tickInterval 15minute\ndateFormat YYYY-MM-DD HH:mm\nA :2024-01-01 09:05, 2h\n'],
  ['tick-too-many', h + 'tickInterval 1millisecond\nA :2024-01-01, 1d\n'],
  ['century', h + 'A :1900-01-01, 36500d\n'],
  ['calendar-units', h + 'A :2024-01-01, 1y\nB :after A, 1M\nC :after B, 1.5M\nD :2024-01-31, 1M\n'],
  ['short-units', h + 'A :a, 2024-01-01, 2w\nB :b, after a, 12h\nC :c, after b, 30m\nD :d, after c, 45s\nE :e, after d, 500ms\nF :after e, 1.5d\n'],
  ['weekend-friday', h + 'excludes weekends\nweekend friday\nA :2024-01-01, 20d\nB :2024-01-10, 2024-01-30\n'],
  ['excludes-includes', h + 'excludes 2024-01-03, monday, 2024-01-10\nincludes 2024-01-08\nA :2024-01-01, 15d\n'],
  ['inclusive-end', h + 'excludes sunday saturday\ninclusiveEndDates\nA :2024-01-01, 2024-01-12\nB :2024-01-01, 10d\n'],
  ['custom-format-excludes', 'gantt\ndateFormat DD-MM-YYYY\nexcludes 06-01-2014\nA :01-01-2014, 10d\nB :2014-01-03, 3d\n'],
  ['ordinal-format', 'gantt\ndateFormat Do MMMM YYYY\nA :5th January 2024, 10d\nB :22nd December 2023, 3d\n'],
  ['month-name-format', 'gantt\ndateFormat MMM D, YYYY\nA :Jan 5, 2024, 10d\n'],
  ['milliseconds-format', 'gantt\ndateFormat YYYY-MM-DDTHH:mm:ss.SSS\nA :2024-01-05T07:05:09.123, 3h\n'],
  ['meridiem-format', 'gantt\ndateFormat h:mm A\nA :9:30 PM, 1h\nB :12:05 AM, 30m\n'],
  ['two-digit-years', 'gantt\ndateFormat YY/M/D\nA :14/1/6, 3d\nB :70/12/31, 2d\n'],
  ['quarter-format', 'gantt\ndateFormat Q YYYY\nA :2 2024, 30d\n'],
  ['milliseconds-epoch', 'gantt\ndateFormat x\nA :1410715640579, 1410715740579\n'],
  ['seconds-epoch', 'gantt\ndateFormat X\nA :1410715640, 1410719240\nB :0, 5\n'],
  ['offset-format', 'gantt\ndateFormat YYYY-MM-DDTHH:mm:ssZ\nA :2024-01-05T10:00:00+00:00, 3h\nB :2024-01-05T10:00:00+05:00, 3h\n'],
  ['javascript-dates', 'gantt\nA :2017-07-20, 3d\nB :July 20 2017, 2d\nC :2017/07/23, 1d\nD :2017-07-20 10:00, 5h\nE :20 Jul 2017 10:00 GMT+0200, 2h\n'],
  ['javascript-zones', 'gantt\nA :Thu Jul 20 2017 10:00:00 GMT-0700 (PDT), 2h\nB :2017-07-20T10:00:00Z, 1h\nC :2017-07-20T10:00:00.5+02:00, 1h\n'],
  ['javascript-legacy', 'gantt\nA :7/20/2017, 2d\nB :20 July, 1d\nC :5, 1d\n'],
  ['top-axis', h + 'topAxis\nA :2024-01-01, 10d\n'],
  ['configured', '---\nconfig:\n  gantt:\n    topAxis: true\n    topPadding: 80\n    gridLineStartPadding: 30\n    barHeight: 30\n    barGap: 8\n    fontSize: 14\n    sectionFontSize: 18\n    leftPadding: 120\n    rightPadding: 40\n    numberSectionStyles: 3\n    axisFormat: "%m/%d"\n---\n' + h + 'section One\nA :2024-01-01, 10d\nsection Two\nB :2024-01-05, 10d\nsection Three\nC :2024-01-07, 2d\nsection Four\nD :2024-01-09, 1d\n'],
  ['configured-ticks', '---\nconfig:\n  gantt:\n    useWidth: 1200\n    tickInterval: 2day\n    weekday: friday\n---\n' + h + 'A :2024-01-01, 20d\n'],
  ['compact', '---\ndisplayMode: compact\n---\n' + h + 'section S\nA :2024-01-01, 10d\nB :2024-01-03, 2d\nC :2024-01-12, 3d\nD :after zz, 1d\nsection T\nE :2024-01-02, 4d\nF :2024-01-02, 4d\n'],
  ['states', h + 'section Plan\nA :done, a, 2024-01-01, 3d\nB :active, b, after a, 4d\nC :crit, done, c, after b, 2d\nD :crit, active, d, after c, 2d\nE :crit, e, after d, 1d\nM :milestone, m, after e, 0d\nCM :crit, milestone, after m, 1d\nV :vert, v, 2024-01-05, 0d\nV2 :vert, 2024-01-06, 1d\n'],
  ['label-placement', h + 'A really quite long task label that does not fit :2024-01-01, 1d\nB :2024-01-02, 20d\nAnother long one at the end of the chart :2024-01-21, 1d\n'],
  ['links', h + 'A :a, 2024-01-01, 3d\nclick a href "https://example.com/a"\nB :b, 2024-01-04, 2d\nclick b call cb(1, 2)\nclick a,b href "/relative/path"\n'],
  ['entities', h + 'title Entities #35; and #quot; here\nsection S #9829;\nTask #35;1 :2024-01-01, 2d\naccTitle: Acc title\naccDescr {\n  line one\n    line two\n}\n'],
  ['references', h + 'A :a, 2024-01-03, 3d\nB :2024-01-01, until a\nC :after nothing, 2d\nD :2024-01-05, until nothing\nE :e, after a B, 1d\n'],
  ['until-start', h + 'A :a, 2024-01-01, 3d\nB :until a, 2d\n'],
  ['no-duration', h + 'A :2024-01-01, 3d\nB :5d\nC :2024-01-03, nonsense\n'],
  ['epoch-today', h + 'todayMarker stroke-width:5px,stroke:#0f0\nA :1970-01-01, 3d\n'],
  ['today-off', h + 'todayMarker off\nA :1969-12-30, 3d\n'],
  ['times-excluded', 'gantt\ndateFormat HH:mm\naxisFormat %H:%M\nexcludes weekends\nA :17:49, 2m\nB :1h\n'],
  ['day-format', 'gantt\ndateFormat D\nA :5, 3d\n'],
  ['empty-sections', h + 'section A\nT :2024-01-01, 2d\nsection B\nsection C\nU :2024-01-02, 2d\n'],
  ['title-only', 'gantt\n  title Only a title\n'],
  ['empty', 'gantt\n'],
  ['long-excluded', h + 'excludes weekends\nA :2024-01-01, 400d\n'],
  ['unicode', h + 'section Fase ü\nTarefa 日本語 :2024-01-01, 3d\nemoji 👋 task :2024-01-02, 2d\n'],
  ['multiline-label', h + 'First<br>second :2024-01-01, 3d\nOne\\ntwo :2024-01-02, 9d\n'],
  ['narrow-host', h + 'section Section\nA task :2024-01-01, 30d\nAnother :2024-01-10, 10d\n', { maxWidth: 420 }],
  ['small-font', h + 'title Small\nsection S\nA :2024-01-01, 5d\n', { fontSize: 12, padding: 0 }],
  ['syntax-header', 'gantt x\nA :2024-01-01, 1d\n'],
  ['syntax-task-data', h + 'A task without data\n'],
  ['syntax-unicode-column', h + '日本語 task\n  B :2024-01-01, 1d\n'],
  ['syntax-click', h + 'A :a, 2024-01-01, 1d\nclick a\n'],
  ['syntax-date-line', h + '2024-01-01 :1d\n'],
  ['semantic-fields', h + 'A :a, b, c, d\n'],
  ['semantic-empty-data', h + 'A :crit, done\n'],
  ['semantic-date', h + 'A :not a date, 1d\n'],
  ['limit-work', h + 'excludes weekends\n' + 'T :2024-01-01, 10950d\n'.repeat(400)],
];
const cases = [
  ...docs.map((text, index) => [`docs-${index}`, text]),
  ...crafted,
];

const renders = [];
const schedules = [];
for (const [id, text, extra] of cases) {
  const options = { ...base, ...extra, now: NOW };
  const entry = { id, source: text };
  if (extra) entry.options = extra;
  try {
    const result = pele.render(text, options);
    Object.assign(entry, { width: result.width, height: result.height, links: result.links, body: body(result.svg) });
  } catch (error) {
    const message = error.message.split('\n').pop();
    entry.error = { code: error.code, line: error.line, column: error.column, message };
  }
  renders.push(entry);
  if (entry.error || text.trimStart().startsWith('---')) continue;
  const model = pele.parse(text);
  const time = (date) => (date === undefined || date === null || Number.isNaN(+date) ? null : +date);
  schedules.push({
    id,
    source: text,
    tasks: model.getTasks(NOW).map((task) => ({
      id: task.id,
      start: time(task.startTime),
      end: time(task.endTime),
      render_end: time(task.renderEndTime),
    })),
  });
}

let state = 20241030;
const next = () => (state = (Math.imul(state, 1664525) + 1013904223) >>> 0);
const times = [
  Date.UTC(2024, 1, 3, 15, 4, 5, 67),
  Date.UTC(2023, 0, 1),
  Date.UTC(2024, 11, 30, 23, 59, 59, 999),
  Date.UTC(2021, 0, 3, 0, 30),
  Date.UTC(1969, 11, 31, 12),
  Date.UTC(1, 0, 1),
  Date.UTC(-271, 5, 15, 7),
  0,
];
for (let k = 0; k < 16; k++) times.push(Date.UTC(1900 + (next() % 250), next() % 12, 1 + (next() % 31), next() % 24, next() % 60, next() % 60, next() % 1000));

const templates = [
  '', 'YYYY-MM-DD', 'DD-MM-YYYY', 'YY/M/D', 'YYYY-MM-DD HH:mm', 'YYYY-MM-DD H:m:s', 'YYYY-MM-DDTHH:mm:ss.SSS', 'h:mm A', 'hh:mm a',
  'MMM D, YYYY', 'Do MMMM YYYY', '[on] YYYY-MM-DD', 'HH:mm', 'YYYY', 'MM', 'D', 'ss', 'Q YYYY', 'dddd YYYY', 'x', 'X',
  'YYYY-MM-DDTHH:mm:ssZ', 'YYYY-MM-DDTHH:mm:ssZZ', 'Y YYY', 'S SS SSS', 'W WW GGGG', 'k kk', 'd dd ddd dddd', '[YYYY] YYYY',
  'w', 'ww', 'wo', 'gggg', 'z', 'zzz', '[w] YYYY', 'L', 'YYYY-ww', 'q', 'DD.MM.YYYY', 'YYYYMMDD', '[x[y]] YYYY',
];
const format = [];
for (const template of templates) for (const t of times) format.push([t, template, dates.format(new Date(t), template) ?? null]);
format.push([Number.NaN, 'YYYY', dates.format(new Date(Number.NaN), 'YYYY')]);

const parse = [];
const parseInputs = [
  ['2014-01-06', 'YYYY-MM-DD'], ['06-01-2014', 'DD-MM-YYYY'], ['14/1/6', 'YY/M/D'], ['70/12/31', 'YY/M/D'],
  ['2014-01-06 17:49', 'YYYY-MM-DD HH:mm'], ['2014-01-06 7:5:9', 'YYYY-MM-DD H:m:s'], ['2014-01-06T07:05:09.123', 'YYYY-MM-DDTHH:mm:ss.SSS'],
  ['9:30 PM', 'h:mm A'], ['12:05 am', 'hh:mm a'], ['12:05 pm', 'hh:mm a'], ['Jan 5, 2024', 'MMM D, YYYY'], ['5th January 2024', 'Do MMMM YYYY'],
  ['22nd December 2024', 'Do MMMM YYYY'], ['on 2024-03-01', '[on] YYYY-MM-DD'], ['17:49', 'HH:mm'], ['2024', 'YYYY'], ['03', 'MM'], ['7', 'D'],
  ['20', 'ss'], ['2 2024', 'Q YYYY'], ['Monday 2024', 'dddd YYYY'], ['2014-1-6', 'YYYY-MM-DD'], ['2014-01-06 7:05:09', 'YYYY-MM-DD H:m:s'],
  ['2014-13-01', 'YYYY-MM-DD'], ['2014-02-30', 'YYYY-MM-DD'], ['202-12-01', 'YYYY-MM-DD'], ['202304', 'YYYYMMDD'], ['2014-01-06 ', 'YYYY-MM-DD'],
  [' 2014-01-06', 'YYYY-MM-DD'], ['2014-01-06x', 'YYYY-MM-DD'], ['0', 'ss'], ['25:00', 'HH:mm'], ['5 January 2024', 'Do MMMM YYYY'],
  ['5th january 2024', 'Do MMMM YYYY'], ['Tuesday 2024', 'dddd YYYY'], ['2014-01-06', ''], ['2014-01-06', 'q'], ['12/25/2020', 'L'],
  ['2020-01', 'YYYY-ww'], ['Invalid Date', ''], ['', 'YYYY-MM-DD'], ['1410715640', 'X'], ['1410715640579', 'x'], ['-5', 'x'],
  ['1410715640.579', 'X'], ['1e3', 'x'], ['', 'x'], ['2024-01-05T10:00:00+00:00', 'YYYY-MM-DDTHH:mm:ssZ'], ['2024-01-05T10:00:00+0000', 'YYYY-MM-DDTHH:mm:ssZZ'],
  ['2024-01-05T10:00:00+05:45', 'YYYY-MM-DDTHH:mm:ssZ'], ['0050-01-01', 'YYYY-MM-DD'], ['1999-12-31T23:59:59.999', 'YYYY-MM-DDTHH:mm:ss.SSS'],
];
for (const [input, template] of parseInputs) parse.push([input, template, dates.parseStrict(input, template, new Date(NOW))?.getTime() ?? null]);
for (const template of templates) {
  for (const t of times.slice(0, 10)) {
    const text = dates.format(new Date(t), template);
    if (text === undefined || text === 'Invalid Date') continue;
    parse.push([text, template, dates.parseStrict(text, template, new Date(NOW))?.getTime() ?? null]);
    const mutated = text.slice(0, next() % (text.length + 1)) + ['1', ' ', 'x', '-', 'Z'][next() % 5] + text.slice(next() % (text.length + 1));
    parse.push([mutated, template, dates.parseStrict(mutated, template, new Date(NOW))?.getTime() ?? null]);
  }
}

const specifiers = [
  '%Y-%m-%d', '%a %A %b %B', '%H:%M:%S.%L', '%I %p', '%e|%-d|%_d|%0e', '%j', '%U %W %V', '%u %w', '%y %G %g', '%q', '%x', '%X', '%c',
  '%f', '%%Y %k %', '%s', '%Q', '%Z', '%-', 'plain', '%d/%m %H:%M',
];
const axis = [];
for (const specifier of specifiers) for (const t of times) axis.push([specifier, t, dates.timeFormat(specifier, new Date(t))]);

const durations = [];
for (const text of ['1.5d', ' 3M ', '500ms', '3dX', '-1d', '2w', '1y', '12h', '30m', '45s', '1.d', '.5d', '1', 'd', '99999999999999999999d', '0d']) {
  const [value, unit] = dates.parseDuration(text);
  durations.push([text, Number.isNaN(value) ? null : value, Number.isNaN(value) ? null : unit]);
}

const add = [];
for (const t of times.slice(0, 12)) {
  for (const text of ['1d', '1.5d', '0.4d', '2w', '0.5w', '1M', '2M', '11M', '1.5M', '1y', '2h', '0.005s', '20ms', '99999999y']) {
    const [amount, unit] = dates.parseDuration(text);
    const result = dates.add(new Date(t), amount, unit).getTime();
    add.push([t, text, Number.isNaN(result) ? null : result]);
  }
}
add.push([Date.UTC(2024, 0, 31), '1M', dates.add(new Date(Date.UTC(2024, 0, 31)), 1, 'M').getTime()]);
add.push([Date.UTC(2024, 1, 29), '1y', dates.add(new Date(Date.UTC(2024, 1, 29)), 1, 'y').getTime()]);

const jsDates = [];
for (const text of [
  '2017-07-20', '2017-07', '2017', '+002017-07-20', '-000001-01-01', '2017-07-20T10:00', '2017-07-20T10:00:00Z', '2017-07-20T10:00:00.5+02:00',
  '2017-07-20T24:00', '2017-07-20T24:01', '2017-07-20T10:00+0530', '2017-07-20 10:00', 'July 20 2017', 'July 20, 2017 10:30 pm', '20 Jul 2017',
  'Thu Jul 20 2017 10:00:00 GMT-0700 (PDT)', '2017/07/20', '7/20/2017', '7/20/17', '20 July', 'July 20', '5', '13', '32', '2017-13-01',
  '2017-02-30', 'foo 2', 'foo2', 'Invalid Date', '', '12:00', '1 2 3 4', 'Dec 25, 1995 13:30 UTC', '1995-12-25 EST', 'Mon, 25 Dec 1995 13:30:00 +0430',
  '2017-07-20T10:00:00.123456', 'tomorrow', '2017.07.20', '20.07.2017', 'Jul-20-2017', '2017-Jul-20', '10:00 2017-07-20', '2017-07-20 10:00:00 GMT+2',
  '1/2', '99', '50 1 1', '2017-07-20T10', '2017-07-20 10', 'Sun 2017', 'am 5', '5 pm 2017', '(note) July 4 1976',
]) {
  const value = new Date(text).getTime();
  jsDates.push([text, Number.isNaN(value) ? null : value]);
}

const day = (y, m, d, hh = 0, mm = 0) => Date.UTC(y, m, d, hh, mm);
const autoTicks = [];
for (const [start, stop, count] of [
  [day(2014, 0, 1), day(2014, 1, 20), 10], [day(2024, 0, 10, 9), day(2024, 0, 10, 18), 10], [day(1900, 0, 1), day(2020, 0, 1), 10],
  [day(2024, 0, 1), day(2024, 6, 31), 7], [0, 71000, 10], [0, 30, 10], [-8.64e15, -8.64e15 + 120000, 4], [day(2024, 0, 1), day(2024, 0, 1, 0, 7), 5],
  [day(2024, 0, 1), day(2024, 0, 3), 6], [day(2010, 5, 1), day(2010, 8, 1), 9], [day(1950, 0, 1), day(1950, 0, 1) + 1, 2], [day(1, 0, 1), day(9999, 0, 1), 10],
]) {
  autoTicks.push([start, stop, count, dates.autoTicks(start, stop, count)]);
}
const intervalTicks = [];
const span = [day(2024, 0, 1), day(2024, 0, 31)];
for (const [interval, weekday, start, stop, limit] of [
  ['1week', 'monday', ...span, 100], ['1week', 'sunday', ...span, 100], ['1week', 'thursday', ...span, 100], ['10day', 'sunday', ...span, 100],
  ['1month', 'sunday', span[0], day(2024, 3, 2), 100], ['6hour', 'sunday', span[0], day(2024, 0, 2), 100],
  ['15minute', 'sunday', day(2024, 0, 1, 9, 5), day(2024, 0, 1, 10), 100], ['2week', 'monday', ...span, 100], ['', 'sunday', ...span, 100],
  ['1decade', 'sunday', ...span, 100], ['1year', 'sunday', ...span, 100], ['0day', 'sunday', ...span, 100], ['01day', 'sunday', ...span, 100],
  ['1 day', 'sunday', ...span, 100], ['1days', 'sunday', ...span, 100], ['1.5day', 'sunday', ...span, 100], ['1second', 'sunday', ...span, 1000],
  ['99999999second', 'sunday', ...span, 1000], ['1day', 'nonsense', ...span, 100], ['2second', 'sunday', 8.64e15 - 1000, 8.64e15, 10],
  ['3month', 'sunday', day(2023, 10, 30), day(2025, 10, 1), 300], ['7day', 'saturday', day(2023, 1, 1), day(2023, 3, 1), 100],
  ['5millisecond', 'sunday', 0, 40, 100], ['30second', 'sunday', 1000, 300000, 100], ['4hour', 'sunday', day(2024, 2, 30), day(2024, 3, 2), 100],
]) {
  intervalTicks.push([interval, weekday, start, stop, limit, dates.intervalTicks(interval, weekday, start, stop, limit) ?? null]);
}

const fixture = {
  upstream_revision: revision,
  time_zone: 'UTC',
  now: NOW,
  palette,
  render_options: base,
  renders,
  schedules,
  dates: { format, parse, axis, durations, add, js_dates: jsDates, auto_ticks: autoTicks, interval_ticks: intervalTicks },
};
fs.writeFileSync(path.join(crate, 'tests/fixtures/gantt.json'), JSON.stringify(fixture, null, 1) + '\n');
console.log(`Wrote ${renders.length} renders, ${schedules.length} schedules, ${format.length + parse.length + axis.length + add.length + jsDates.length} date oracles.`);
