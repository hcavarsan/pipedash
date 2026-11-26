import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { RunLogNode } from '../../../types/logs'

import { type JobListRow, jobListRows, resolveLogGroups, toggleStage } from './logTree'

const node = (id: string, overrides: Partial<RunLogNode> = {}): RunLogNode => ({
  id,
  parent_id: null,
  kind: 'job',
  name: id,
  status: 'success',
  started_at: null,
  finished_at: null,
  duration_seconds: null,
  log: 'ready',
  web_url: null,
  depth: 0,
  navigable: true,
  ...overrides,
})

const tree = {
  nodes: [node('a'), node('b'), node('lone')],
  groups: [
    { stage: 'build', node_ids: ['a', 'b'], failed_first_node_ids: ['b', 'a'], skipped_node_ids: [] },
    { stage: null, node_ids: ['lone'], failed_first_node_ids: ['lone'], skipped_node_ids: [] },
  ],
}

test('groups resolve backend node ids in the requested order', () => {
  assert.deepEqual(
    resolveLogGroups(tree).map((group) => [group.stage, group.nodes.map((n) => n.id)]),
    [
      ['build', ['a', 'b']],
      [null, ['lone']],
    ]
  )
  assert.deepEqual(
    resolveLogGroups(tree, true).map((group) => group.nodes.map((n) => n.id)),
    [['b', 'a'], ['lone']]
  )
})

const staged = {
  nodes: [
    node('build', { kind: 'stage', log: null }),
    node('lint', { parent_id: 'build', depth: 1, status: 'skipped' }),
    node('compile', { parent_id: 'build', depth: 1, status: 'failed' }),
    node('docs', { parent_id: 'build', depth: 1, status: 'skipped' }),
    node('deploy', { kind: 'stage', log: null }),
    node('ship', { parent_id: 'deploy', depth: 1 }),
  ],
  groups: [
    { stage: 'build', node_ids: ['compile'], failed_first_node_ids: ['compile'], skipped_node_ids: ['lint', 'docs'] },
    { stage: 'deploy', node_ids: ['ship'], failed_first_node_ids: ['ship'], skipped_node_ids: [] },
  ],
}

const rowNames = (rows: JobListRow[]) =>
  rows.map((row) => (row.kind === 'node' ? row.node.id : `${row.count} skipped ${row.open ? 'open' : 'folded'}`))

test('skipped jobs fold into one row at the end of their stage and open in place', () => {
  const key = JSON.stringify('build')

  assert.deepEqual(rowNames(jobListRows(staged, new Set(), null)), [
    'build',
    'compile',
    '2 skipped folded',
    'deploy',
    'ship',
  ])
  assert.deepEqual(rowNames(jobListRows(staged, toggleStage(new Set(), key), null)), [
    'build',
    'lint',
    'compile',
    'docs',
    '2 skipped open',
    'deploy',
    'ship',
  ])
  assert.equal(toggleStage(toggleStage(new Set(), key), key).size, 0)
})

test('a selected skipped job keeps its fold open', () => {
  assert.deepEqual(rowNames(jobListRows(staged, new Set(), 'docs')).slice(0, 5), [
    'build',
    'lint',
    'compile',
    'docs',
    '2 skipped open',
  ])
})

test('a run with no skipped jobs lists every job as before', () => {
  assert.deepEqual(rowNames(jobListRows(tree, new Set(), null)), ['a', 'b', 'lone'])
})
