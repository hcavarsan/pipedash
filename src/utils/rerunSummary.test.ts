import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { PipelineRun } from '../types'

import { buildRerunSummary, canRerunFailedOnly } from './rerunSummary'

const run = (overrides: Partial<PipelineRun> = {}): PipelineRun => ({
  id: 'r1',
  pipeline_id: 'github__1__acme/app/ci',
  run_number: 42,
  status: 'failed',
  started_at: '2026-10-28T10:00:00Z',
  concluded_at: null,
  duration_seconds: null,
  logs_url: '',
  commit_sha: 'abcdef1234567890',
  commit_message: 'Fix flaky test\n\nLong body',
  branch: 'main',
  actor: 'octocat',
  ...overrides,
})

const context = {
  pipeline: { name: 'CI', repository: 'acme/app', provider_type: 'github', metadata: {} },
  providerName: 'GitHub',
}

test('summary lists pipeline, repository, provider, run, branch and short commit with first message line', () => {
  assert.deepEqual(buildRerunSummary(run(), context).fields, [
    { label: 'Pipeline', value: 'CI' },
    { label: 'Repository', value: 'acme/app' },
    { label: 'Provider', value: 'GitHub' },
    { label: 'Run', value: '#42' },
    { label: 'Branch', value: 'main' },
    { label: 'Commit', value: 'abcdef1 Fix flaky test' },
  ])
})

test('summary names where a deployment pipeline acts', () => {
  const target = (provider_type: string, metadata: Record<string, unknown>) =>
    buildRerunSummary(run(), { ...context, pipeline: { ...context.pipeline, provider_type, metadata } }).fields.at(-1)

  assert.deepEqual(target('argocd', { destination_cluster: 'prod', destination_namespace: 'shop' }), {
    label: 'Destination',
    value: 'prod / shop',
  })
  assert.deepEqual(target('fluxcd', { namespace: 'apps' }), { label: 'Namespace', value: 'apps' })
  assert.deepEqual(target('github', {}), { label: 'Commit', value: 'abcdef1 Fix flaky test' })
})

test('summary skips branch and commit when the run does not carry them', () => {
  const { fields } = buildRerunSummary(run({ branch: null, commit_sha: null, commit_message: null }), context)

  assert.deepEqual(
    fields.map(({ label }) => label),
    ['Pipeline', 'Repository', 'Provider', 'Run']
  )
})

test('summary commit falls back to whichever of sha and message exists', () => {
  const only = (overrides: Partial<PipelineRun>) =>
    buildRerunSummary(run(overrides), context).fields.find(({ label }) => label === 'Commit')?.value

  assert.equal(only({ commit_message: null }), 'abcdef1')
  assert.equal(only({ commit_sha: null }), 'Fix flaky test')
})

test('inputs hide the branch ref and empty values and keep the rest', () => {
  const { inputs } = buildRerunSummary(
    run({ inputs: { ref: 'main', env: 'staging', empty: '', none: null, dry: false, matrix: { os: 'linux' } } }),
    context
  )

  assert.deepEqual(inputs, [
    { label: 'env', value: 'staging' },
    { label: 'dry', value: 'false' },
    { label: 'matrix', value: '{"os":"linux"}' },
  ])
})

test('a ref input that differs from the branch is shown', () => {
  assert.deepEqual(buildRerunSummary(run({ inputs: { ref: 'release' } }), context).inputs, [
    { label: 'ref', value: 'release' },
  ])
})

test('no inputs yields an empty list', () => {
  assert.deepEqual(buildRerunSummary(run(), context).inputs, [])
})

test('failed-only rerun is offered only when the provider flags the run', () => {
  assert.equal(canRerunFailedOnly(run({ metadata: { can_rerun_failed_jobs: true } })), true)
  assert.equal(canRerunFailedOnly(run({ metadata: { can_rerun_failed_jobs: 'yes' } })), false)
  assert.equal(canRerunFailedOnly(run()), false)
})
