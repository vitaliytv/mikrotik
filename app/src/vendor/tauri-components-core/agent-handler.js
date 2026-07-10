import { runAgent } from './llm.js'

// Agent-gateway handlers — start / resume / approve the agent loop on behalf of a
// caller (human via UI or agent via MCP). They own the journal lifecycle and
// return a structured result contract.
//
// All collaborators are injected (no module-level singletons): `dispatch`,
// `journal`, the scoped `tools` manifest, the `gate` decision fn, `buildSystem`
// (turns optional grounding context into a system prompt) and `grounding` (an
// optional read tool whose output grounds the prompt). createAgentKit wires
// these from an app catalog; tests pass fakes.
//
// Trust: destructive tool calls from an agent pause as `needs_approval` (the
// loop never executes them); a human approves later via handleApprove.

const QUESTION_RE = /\?\s*$/

/**
 * @param {string} text candidate text
 * @returns {boolean} true when text ends with a question mark
 */
function isQuestion(text) {
  return typeof text === 'string' && QUESTION_RE.test(text.trim())
}

/**
 * Derive the structured result fields from a runAgent result.
 * @param {object} result runAgent result
 * @returns {{ status: string, summary: string|null, question: string|null, pendingApproval: object|null }} fields
 */
function finalize(result) {
  const question = isQuestion(result.content) ? result.content : null
  let status = 'done'
  if (result.stopped === 'needs_approval') status = 'needs_approval'
  else if (result.stopped === 'max_steps') status = 'partial'
  else if (question) status = 'needs_clarification'
  return {
    status,
    summary: question ? null : (result.content || null),
    question,
    pendingApproval: result.pendingApproval ?? null,
  }
}

/**
 * Run the loop (fresh or resumed), persist to the journal, return the envelope.
 * @param {{ requestId: string, runArgs: object, baseActions: object[], journal: object }} ctx run context
 * @returns {Promise<object>} structured result envelope
 */
async function runAndJournal({ requestId, runArgs, baseActions, journal }) {
  let result
  try {
    result = await runAgent(runArgs)
  }
  catch (error) {
    await journal.update(requestId, { status: 'failed', error: String(error?.message ?? error) })
    return { requestId, status: 'failed', summary: null, actions: baseActions, question: null, pendingApproval: null }
  }

  const fields = finalize(result)
  const actions = [...baseActions, ...result.trace]
  await journal.update(requestId, { ...fields, messages: result.messages, actions })
  return { requestId, ...fields, actions }
}

/**
 * Fetch optional grounding context via a read tool; {} when not configured / on failure.
 * @param {Function} dispatch tool dispatcher
 * @param {{ tool: string, key?: string, fallback?: unknown }} [grounding] grounding config
 * @returns {Promise<object>} context object keyed by grounding.key (default "grounding")
 */
async function groundingContext(dispatch, grounding) {
  if (!grounding?.tool) return {}
  const key = grounding.key ?? 'grounding'
  const fallback = grounding.fallback ?? []
  try {
    const res = await dispatch(grounding.tool, {})
    return { [key]: res?.ok ? res.output : fallback }
  }
  catch {
    return { [key]: fallback }
  }
}

/**
 * Start a new agent request.
 * @param {{ intent: string, actor: object, chat: Function, dispatch: Function, journal: object, tools: object[], gate: Function, buildSystem?: Function, grounding?: object }} opts request parameters
 * @returns {Promise<object>} structured result envelope
 */
export async function handleRequest({ intent, actor, chat, dispatch, journal, tools, gate, buildSystem, grounding }) {
  const id = await journal.create({ intent, actor })
  await journal.update(id, { status: 'running' })
  const ctx = await groundingContext(dispatch, grounding)
  const system = buildSystem ? buildSystem(ctx) : undefined
  return runAndJournal({
    requestId: id,
    baseActions: [],
    journal,
    runArgs: { system, prompt: intent, chat, dispatch, tools, gate },
  })
}

/**
 * Resume a conversation with a follow-up / clarification answer.
 * @param {{ requestId: string, message: string, actor: object, chat: Function, dispatch: Function, journal: object, tools: object[], gate: Function }} opts resume parameters
 * @returns {Promise<object>} updated result envelope
 */
export async function handleRespond({ requestId, message, chat, dispatch, journal, tools, gate }) {
  let record
  try {
    record = await journal.load(requestId)
  }
  catch {
    return { requestId, status: 'failed', summary: null, actions: [], question: 'Request not found.', pendingApproval: null }
  }

  if (record.status === 'running' || !Array.isArray(record.messages) || record.messages.length === 0) {
    return { requestId, status: record.status, summary: record.summary, actions: record.actions, question: null, pendingApproval: record.pendingApproval ?? null }
  }

  await journal.update(requestId, { status: 'running' })
  return runAndJournal({
    requestId,
    baseActions: record.actions ?? [],
    journal,
    runArgs: { messages: [...record.messages, { role: 'user', content: message }], chat, dispatch, tools, gate },
  })
}

/**
 * Approve (or reject) a pending destructive action. Executes with the approver's
 * (human) authority via the injected dispatch — no gate.
 * @param {{ requestId: string, approve: boolean, dispatch: Function, journal: object }} opts approval parameters
 * @returns {Promise<object>} updated result envelope
 */
export async function handleApprove({ requestId, approve, dispatch, journal }) {
  let record
  try {
    record = await journal.load(requestId)
  }
  catch {
    return { requestId, status: 'failed', summary: null, actions: [], question: 'Request not found.', pendingApproval: null }
  }

  if (record.status !== 'needs_approval' || !record.pendingApproval) {
    return { requestId, status: record.status, summary: record.summary, actions: record.actions ?? [], question: null, pendingApproval: null }
  }

  if (!approve) {
    const fields = { status: 'rejected', summary: 'Rejected by human.', pendingApproval: null }
    await journal.update(requestId, fields)
    return { requestId, ...fields, actions: record.actions ?? [], question: null }
  }

  const { tool, input } = record.pendingApproval
  await journal.update(requestId, { status: 'running' })
  const envelope = await dispatch(tool, input)
  const actions = [...(record.actions ?? []), { tool, input, envelope }]

  if (!envelope.ok) {
    // Execution failed (e.g. transient) — stay needs_approval so the human can
    // retry after fixing; keep pendingApproval, record the error and the attempt.
    const error = envelope.error?.message ?? 'execution failed'
    await journal.update(requestId, { status: 'needs_approval', actions, error })
    return { requestId, status: 'needs_approval', summary: null, actions, error, question: null, pendingApproval: record.pendingApproval }
  }

  const summary = `Approved: ${tool} executed.`
  await journal.update(requestId, { status: 'done', actions, summary, error: null, pendingApproval: null })
  return { requestId, status: 'done', summary, actions, question: null, pendingApproval: null }
}
