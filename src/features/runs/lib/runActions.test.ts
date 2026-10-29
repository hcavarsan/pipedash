import assert from 'node:assert/strict'
import { test } from 'node:test'

import { pipelineFixture } from '../../../testing/fixtures'
import type { PipelineRun } from '../../../types/domain'
import { getPipelineLabels } from '../../../utils/pipelinePresentation'

import { buildRunActions, type RunActionsValue, swipeActionsOf } from './runActions'

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
  ...overrides,
})

const context = (overrides: Partial<RunActionsValue> = {}): RunActionsValue => ({
  pipeline: pipelineFixture({ id: 'p1', provider_type: 'github' }),
  labels: getPipelineLabels('github'),
  isFlux: false,
  canRerun: true,
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
  const awaiting = run({ status: 'running', metadata: { awaiting_review: true } })
  const value = context({ reviewingRun: 9 })
  const actions = buildRunActions(value, awaiting)
  assert.equal(actions.find((action) => action.id === 'approve')?.disabled, true)
  assert.equal(actions.find((action) => action.id === 'reject')?.disabled, true)
  assert.deepEqual(swipeKeys(value, awaiting), [])
})
