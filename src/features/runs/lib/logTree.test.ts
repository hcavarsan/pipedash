import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { RunLogNode } from '../../../types/logs'

import { resolveLogGroups } from './logTree'

const node = (id: string): RunLogNode => ({
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
})

const tree = {
  nodes: [node('a'), node('b'), node('lone')],
  groups: [
    { stage: 'build', node_ids: ['a', 'b'], failed_first_node_ids: ['b', 'a'] },
    { stage: null, node_ids: ['lone'], failed_first_node_ids: ['lone'] },
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
