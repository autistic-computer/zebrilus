import type { HttpResponse, PermissionRequestDecision, Register } from 'claude-code'

type Event = Record<string, unknown>
type Command = { id: number; op: string; text?: string; value?: string }
type Checked = { id: string; tool: string; input: string; agent?: string }
type Reply = { decision?: PermissionRequestDecision; pending?: boolean }
type Call = (path: string, method: string, body?: string) => Promise<HttpResponse>
type Polled = { commands: Command[]; watched?: boolean }

const ASK_LIMIT = 30 * 60 * 1000
const RETRY = 1000
const MAX_QUEUE = 2000
const STREAM = new Set(['text', 'thinking', 'used'])

export const register: Register = (on) => {
  const instance = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
  let call: Call | undefined
  let linked = false
  let watched = false
  let sending = false
  let queue: Event[] = []
  let turn: string | undefined
  let asked = 0
  let checked: Checked[] = []
  const lengths = new Map<string, number>()

  const post = async (events: Event[]) => {
    if (call === undefined) return false
    try {
      const response = await call('/events', 'POST', JSON.stringify(events))
      if (response.ok) watched = JSON.parse(response.text).watched === true
      return response.ok
    } catch {
      return false
    }
  }

  const flush = async () => {
    if (sending || queue.length === 0) return
    sending = true
    const batch = queue
    queue = []
    if (!(await post(batch))) {
      linked = false
      queue = []
    }
    sending = false
    if (linked && queue.length > 0) void flush()
  }

  const emit = (event: Event) => {
    if (!linked || (!watched && STREAM.has(String(event.t)))) return
    if (queue.length >= MAX_QUEUE) {
      linked = false
      queue = []
      return
    }
    queue.push({ ...event, i: instance, at: Date.now() })
    void flush()
  }

  const take = (tool: string, input: string, agent?: string) => {
    const same = checked.filter((entry) => entry.tool === tool && entry.agent === agent)
    const found = same.findLast((entry) => entry.input === input) ?? same.at(-1)
    if (found !== undefined) checked = checked.filter((entry) => entry !== found)
    return found
  }

  on('session.start', async ($, e, next) => {
    const result = await next(e)
    if (!e.isInteractive) return result
    const socket = await $.env.get('ZZ_LENS_SOCKET')
    const pane = (await $.env.get('ZZ_PANE'))?.replace(/^%/, '')
    if (!socket || !pane) return result
    const channel: Call = (path, method, body) =>
      $.http.fetch(`http://zz${path}${path.includes('?') ? '&' : '?'}pane=${pane}`, {
        method,
        headers: { 'content-type': 'application/json' },
        body,
        socketPath: socket,
      })
    const hello = async (): Promise<Event> => ({
      t: 'hello',
      i: instance,
      at: Date.now(),
      session: await $.session.id(),
      model: await $.session.model(),
      turn: turn ?? null,
    })
    const restarted = call !== undefined
    call = channel
    let polling = false

    const run = async (command: Command) => {
      try {
        if (command.op === 'prompt' && command.text !== undefined) {
          const submitted = await $.prompt.submit({ text: command.text, asUser: true })
          emit({ t: 'ack', id: command.id, ok: submitted.drop === undefined, error: submitted.drop })
        } else if (command.op === 'abort') {
          if (turn !== undefined) await $.turn.abort({ turnId: turn })
          emit({ t: 'ack', id: command.id, ok: true })
        } else if (command.op === 'model' && command.value !== undefined) {
          const set = await $.config.set({ key: 'model', value: command.value })
          emit({ t: 'ack', id: command.id, ok: set.deny === undefined, error: set.deny })
          emit({ t: 'model', model: await $.session.model() })
        } else {
          emit({ t: 'ack', id: command.id, ok: false, error: `unknown command ${command.op}` })
        }
      } catch (error) {
        emit({ t: 'ack', id: command.id, ok: false, error: String(error) })
      }
    }

    const poll = async () => {
      if (polling) return
      polling = true
      try {
        for (;;) {
          if (!linked) {
            if (!(await post([await hello()]))) break
            linked = true
          }
          const response = await channel(`/commands?instance=${instance}`, 'GET')
          if (!response.ok) break
          const polled: Polled = JSON.parse(response.text)
          watched = polled.watched === true
          for (const command of polled.commands) void run(command)
        }
      } catch {}
      linked = false
      queue = []
      polling = false
    }

    if (restarted && linked) void post([await hello()])
    void poll()
    $.clock.every(RETRY, () => {
      void poll()
    })
    return result
  })

  on('session.end', async ($, e, next) => {
    turn = undefined
    emit({ t: 'end', reason: e.reason })
    return next(e)
  })

  on('turn.start', async ($, e, next) => {
    if (e.agentId === undefined) {
      turn = e.turnId
      lengths.clear()
      emit({ t: 'turn', turn: e.turnId })
    }
    return next(e)
  })

  on('turn.step', async function* ($, e, next) {
    emit({ t: 'step', turn: e.turnId, step: e.index, agent: e.agentId })
    const stream = next(e)
    if (e.agentId !== undefined || call === undefined) return yield* stream
    const chunks = stream[Symbol.asyncIterator]()
    for (;;) {
      const { value, done } = await chunks.next()
      if (done) return value
      if (value.kind === 'text' || value.kind === 'thinking') {
        const key = `${e.turnId}:${e.index}:${value.index}`
        const off = lengths.get(key) ?? 0
        lengths.set(key, off + value.text.length)
        emit({ t: value.kind, turn: e.turnId, step: e.index, index: value.index, off, text: value.text })
      }
      yield value
    }
  })

  on('turn.complete', async ($, e, next) => {
    if (e.agentId === undefined) turn = undefined
    emit({ t: 'done', turn: e.turnId, agent: e.agentId, reason: e.reason })
    return next(e)
  })

  on('tool.check', async ($, e, next) => {
    const result = await next(e)
    if (call !== undefined && result.decision === 'ask' && e.tool_use_id !== undefined) {
      checked.push({ id: e.tool_use_id, tool: e.tool, input: JSON.stringify(e.input), agent: e.agentId })
      if (checked.length > 32) checked.shift()
    }
    return result
  })

  on('classic.PermissionRequest', async ($, e, next) => {
    const result = await next(e)
    const channel = call
    const found = take(e.tool_name, JSON.stringify(e.tool_input), e.agent_id)
    if (!linked || !watched || channel === undefined || result.decision !== undefined) return result
    asked += 1
    const body = JSON.stringify({
      t: 'ask',
      i: instance,
      at: Date.now(),
      key: `${instance}:${asked}`,
      tool: e.tool_name,
      input: e.tool_input,
      id: found?.id,
      agent: e.agent_id,
      suggestions: e.permission_suggestions ?? [],
    })
    const until = Date.now() + ASK_LIMIT
    while (Date.now() < until && !next.signal.aborted) {
      let reply: Reply
      try {
        const response = await channel('/ask', 'POST', body)
        if (!response.ok) return result
        reply = JSON.parse(response.text)
      } catch {
        return result
      }
      if (reply.decision !== undefined) return { ...result, decision: reply.decision }
      if (reply.pending !== true) return result
    }
    return result
  })

  on('classic.PostToolUse', async ($, e, next) => {
    emit({ t: 'used', id: e.tool_use_id })
    return next(e)
  })

  on('classic.PostToolUseFailure', async ($, e, next) => {
    emit({ t: 'used', id: e.tool_use_id })
    return next(e)
  })

  on('classic.PermissionDenied', async ($, e, next) => {
    emit({ t: 'used', id: e.tool_use_id })
    return next(e)
  })
}
