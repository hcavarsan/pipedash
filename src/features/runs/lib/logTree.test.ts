import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { RunLogNode } from '../../../types/logs'

import { groupLogNodes } from './logTree'

const node = (id: string, patch: Partial<RunLogNode> = {}): RunLogNode => ({
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
  ...patch,
})

test('jobs group under their nearest stage and failed jobs can lead', () => {
  const nodes = [
    node('build', { kind: 'stage', log: null }),
    node('a', { parent_id: 'build' }),
    node('b', { parent_id: 'build', status: 'failed' }),
    node('step', { parent_id: 'b', kind: 'step', log: null }),
    node('lone'),
  ]

  assert.deepEqual(
    groupLogNodes(nodes, true).map((group) => [group.stage, group.nodes.map((n) => n.id)]),
    [
      ['build', ['b', 'a']],
      [null, ['lone']],
    ]
  )
  const [first] = groupLogNodes(nodes)
  assert.ok(first)
  assert.deepEqual(
    first.nodes.map((n) => n.id),
    ['a', 'b']
  )
})

test('empty input, orphan parents and duplicate stage names group predictably', () => {
  assert.deepEqual(groupLogNodes([]), [])
  const nodes = [
    node('s1', { kind: 'stage', log: null, name: 'build' }),
    node('s2', { kind: 'stage', log: null, name: 'build' }),
    node('a', { parent_id: 's1' }),
    node('b', { parent_id: 's2' }),
    node('orphan', { parent_id: 'missing' }),
  ]

  assert.deepEqual(
    groupLogNodes(nodes).map((group) => [group.stage, group.nodes.map((n) => n.id)]),
    [
      ['build', ['a', 'b']],
      [null, ['orphan']],
    ]
  )
})

test('cyclic parent chains terminate and leave the job ungrouped', () => {
  const nodes = [
    node('self', { parent_id: 'self', kind: 'step' }),
    node('x', { parent_id: 'y', kind: 'step' }),
    node('y', { parent_id: 'x', kind: 'step' }),
  ]

  assert.deepEqual(
    groupLogNodes(nodes).map((group) => [group.stage, group.nodes.map((n) => n.id)]),
    [[null, ['self', 'x', 'y']]]
  )
})
