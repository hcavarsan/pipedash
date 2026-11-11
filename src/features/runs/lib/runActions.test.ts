import assert from 'node:assert/strict'
import { test } from 'node:test'

import { pipelineFixture } from '../../../testing/fixtures'
import { pluginPresentationFixture, runActionsFixture, runPresentationFixture } from '../../../testing/presentation'
import type { PipelineRun } from '../../../types/pipelines'

import { buildRunActions, type RunActionsValue, swipeActionsOf } from './runActions'

const blocked = { allowed: false, reason: null }
const allowed = { allowed: true, reason: null }

const run = (overrides: Partial<PipelineRun> = {}): PipelineRun => ({
  id: 'run-7',
  pipeline_id: 'p1',
  run_number: 7,
  status: 'success',
  started_at: '2026-10-25T00:00:00Z',
  concluded_at: null,
  duration_seconds: null,
  logs_url: '',
  commit_sha: null,
  commit_message: null,
  branch: null,
  actor: null,
  ...runPresentationFixture(),
  actions: runActionsFixture({ cancel: blocked, force_cancel: blocked, review: blocked, rerun_failed: blocked }),
  ...overrides,
})

const context = (overrides: Partial<RunActionsValue> = {}): RunActionsValue => ({
  pipeline: pipelineFixture({ id: 'p1', provider_type: 'stub' }),
  presentation: pluginPresentationFixture(),
  labels: pluginPresentationFixture().terminology,
  canCancel: true,
  cancellingRun: null,
  reviewingRun: null,
  rerunningFailedRun: null,
  rerunningRuns: new Set<number>(),
  watch: { supported: false, available: false, reason: undefined, busy: false, disabled: true },
  startWatch: () => undefined,
  openRun: () => undefined,
  openTrigger: () => undefined,
  cancel: async () => undefined,
  review: async () => undefined,
  requestRerun: () => undefined,
  rerunTarget: null,
  closeRerun: () => undefined,
  confirmRerun: async () => undefined,
  triggerBlockedReason: undefined,
  ...overrides,
})

const swipeKeys = (value: RunActionsValue, target: PipelineRun) =>
  swipeActionsOf(buildRunActions(value, target)).map((action) => action.key)

test('a queued run can be cancelled from history', async () => {
  let cancelled: PipelineRun | undefined
  const pending = run({
    id: 'run-9001',
    run_number: 9001,
    status: 'pending',
    actions: runActionsFixture({ cancel: allowed, force_cancel: blocked, rerun: blocked }),
  })
  const value = context({
    canCancel: true,
    cancel: async (target) => {
      cancelled = target
    },
  })
  const cancel = buildRunActions(value, pending).find((action) => action.id === 'cancel')

  assert.equal(cancel?.label, 'Cancel run #9001')
  assert.equal(cancel?.disabled, false)
  assert.ok(swipeKeys(value, pending).includes('cancel'))
  await cancel?.execute()
  assert.equal(cancelled, pending)
})

test('an idle finished run offers rerun on every surface', () => {
  const actions = buildRunActions(context(), run())
  assert.equal(actions.find((action) => action.id === 'rerun')?.disabled, false)
  assert.deepEqual(swipeKeys(context(), run()), ['rerun'])
})

test('a rerun in flight disables the rerun action and removes its swipe', () => {
  const value = context({ rerunningRuns: new Set([7]) })
  const rerun = buildRunActions(value, run()).find((action) => action.id === 'rerun')
  assert.equal(rerun?.disabled, true)
  assert.equal(rerun?.pending, true)
  assert.deepEqual(swipeKeys(value, run()), [])
})

test('a failed-jobs rerun in flight blocks rerun the same way', () => {
  const value = context({ rerunningFailedRun: 7 })
  const rerun = buildRunActions(value, run()).find((action) => action.id === 'rerun')
  assert.equal(rerun?.disabled, true)
  assert.deepEqual(swipeKeys(value, run()), [])
})

test('a pending review disables both review decisions', () => {
  const awaiting = run({
    status: 'running',
    actions: runActionsFixture({ cancel: blocked, review: allowed, rerun: blocked }),
  })
  const value = context({ reviewingRun: 9 })
  const actions = buildRunActions(value, awaiting)
  assert.equal(actions.find((action) => action.id === 'approve')?.disabled, true)
  assert.equal(actions.find((action) => action.id === 'reject')?.disabled, true)
  assert.deepEqual(swipeKeys(value, awaiting), [])
})

test('force cancel is an option of the one cancel action, never a second action', async () => {
  let calls = 0
  const value = context({
    cancel: async () => {
      calls += 1
    },
  })
  const running = run({
    status: 'running',
    actions: runActionsFixture({ cancel: allowed, force_cancel: allowed, rerun: blocked }),
  })
  const forceOnly = run({
    status: 'running',
    actions: runActionsFixture({ cancel: blocked, force_cancel: allowed, rerun: blocked }),
  })

  for (const target of [running, forceOnly]) {
    const cancels = buildRunActions(value, target).filter((action) => action.label.includes('ancel'))

    assert.deepEqual(
      cancels.map((action) => [action.id, action.label]),
      [['cancel', 'Cancel run #7']]
    )
  }

  await buildRunActions(value, forceOnly)
    .find((action) => action.id === 'cancel')
    ?.execute()
  assert.equal(calls, 1)
  assert.deepEqual(swipeKeys(value, running), ['cancel'])
})
