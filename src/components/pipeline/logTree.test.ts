import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { RunLogNode } from '../../types/logs'

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
