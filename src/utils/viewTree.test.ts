import assert from 'node:assert/strict'
import { test } from 'node:test'

import { pipelineViewFixture } from '../testing/fixtures'
import type { PipelineView } from '../types/views'
import { childViews, reorderSlots, rootViewOf, viewAncestors } from './viewTree'

const view = (id: string, parent: string | null = null) => pipelineViewFixture({ id, parent, kind: 'custom' })

const views = [view('a'), view('a1', 'a'), view('a1x', 'a1'), view('a2', 'a'), view('b')]

const ids = (list: PipelineView[]) => list.map((v) => v.id)

test('ancestors and root follow the parent chain', () => {
  assert.deepEqual(ids(viewAncestors(views, 'a1x')), ['a', 'a1'])
  assert.equal(rootViewOf(views, 'a1x')?.id, 'a')
  assert.equal(rootViewOf(views, 'b')?.id, 'b')
  assert.deepEqual(ids(childViews(views, 'a')), ['a1', 'a2'])
})

test('reorderSlots permutes only the moved ids within their slots', () => {
  assert.deepEqual(reorderSlots(['a', 'a1', 'b', 'c'], ['c', 'a', 'b']), ['c', 'a1', 'a', 'b'])
})
