import assert from 'node:assert/strict'
import { test } from 'node:test'

import { pipelineViewFixture } from '../../../testing/fixtures'
import type { PinnedItem } from '../../../types/views'
import { ancestorKeys, buildViewNodes, isPinned, type NavNode, pinTo } from './navTree'

const view = (id: string, name: string, parent: string | null = null) =>
  pipelineViewFixture({ id, name, parent, kind: 'custom' })

const views = [
  view('prod', 'Production'),
  view('web', 'Web', 'prod'),
  view('eu', 'Europe', 'prod'),
  view('berlin', 'Berlin', 'eu'),
  view('stage', 'Staging'),
]

const names = (nodes: NavNode[]): unknown[] =>
  nodes.map((node) => (node.children.length ? { [node.name]: names(node.children) } : node.name))

test('sub-views nest under their parent view', () => {
  assert.deepEqual(names(buildViewNodes(views)), [{ Production: ['Web', { Europe: ['Berlin'] }] }, 'Staging'])
  const [production] = buildViewNodes(views)
  assert.ok(production)
  assert.deepEqual(
    production.children.map((node) => node.key),
    ['view:web', 'view:eu']
  )
})

test('ancestor keys list the chain above a view, or null when absent', () => {
  const nodes = buildViewNodes(views)

  assert.deepEqual(ancestorKeys(nodes, 'view:eu'), ['view:prod'])
  assert.deepEqual(ancestorKeys(nodes, 'view:stage'), [])
  assert.equal(ancestorKeys(nodes, 'view:nope'), null)
})

test('pins link to their view or repository search and are found by kind and ref', () => {
  const pins: PinnedItem[] = [
    { kind: 'view', ref: 'prod', name: 'Production' },
    { kind: 'repository', ref: 'acme/api server', name: 'api server' },
  ]

  assert.deepEqual(pins.map(pinTo), ['/pipelines?view=prod', '/pipelines?search=acme%2Fapi%20server'])
  assert.equal(isPinned(pins, 'repository', 'acme/api server'), true)
  assert.equal(isPinned(pins, 'view', 'acme/api server'), false)
})
