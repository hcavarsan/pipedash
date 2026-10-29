import assert from 'node:assert/strict'
import { test } from 'node:test'

import { pipelineViewFixture, viewPipelineFixture } from '../testing/fixtures'
import type { PipelineView, ViewMembership } from '../types'
import {
  canAddSubView,
  childViews,
  directMembers,
  eligibleViewParents,
  flattenViewTree,
  reorderSlots,
  rootViewOf,
  setDirectMembership,
  viewAncestors,
  viewDepth,
  viewMemberIds,
} from './viewTree'

const view = (id: string, parent: string | null = null, kind: PipelineView['kind'] = 'custom') =>
  pipelineViewFixture({ id, parent, kind })

const views = [
  view('all', null, 'all'),
  view('a'),
  view('a1', 'a'),
  view('a1x', 'a1'),
  view('a2', 'a'),
  view('b'),
  view('cfg', null, 'config'),
]

const ids = (list: PipelineView[]) => list.map((v) => v.id)

const pick = (id: string): PipelineView => {
  const found = views.find((v) => v.id === id)

  assert.ok(found)

  return found
}

test('depth, ancestors and root follow the parent chain', () => {
  assert.equal(viewDepth(views, 'a'), 1)
  assert.equal(viewDepth(views, 'a1x'), 3)
  assert.deepEqual(ids(viewAncestors(views, 'a1x')), ['a', 'a1'])
  assert.equal(rootViewOf(views, 'a1x')?.id, 'a')
  assert.equal(rootViewOf(views, 'b')?.id, 'b')
  assert.deepEqual(ids(childViews(views, 'a')), ['a1', 'a2'])
})

test('flattenViewTree returns pre-order even when the list order was disturbed', () => {
  const shuffled = [pick('a'), pick('b'), pick('a1'), pick('a1x'), pick('a2')]

  assert.deepEqual(ids(flattenViewTree(shuffled)), ['a', 'a1', 'a1x', 'a2', 'b'])
})

test('only custom views shallower than the maximum depth accept sub-views', () => {
  assert.equal(canAddSubView(views, pick('a')), true)
  assert.equal(canAddSubView(views, pick('a1')), true)
  assert.equal(canAddSubView(views, pick('a1x')), false)
  assert.equal(canAddSubView(views, pick('all')), false)
  assert.equal(canAddSubView(views, pick('cfg')), false)
})

test('a new view can sit under custom views of depth 1 or 2', () => {
  assert.deepEqual(ids(eligibleViewParents(views, null)), ['a', 'a1', 'a2', 'b'])
})

test('a view cannot move into itself, its subtree, or past the maximum depth', () => {
  assert.deepEqual(ids(eligibleViewParents(views, 'a')), [])
  assert.deepEqual(ids(eligibleViewParents(views, 'a1')), ['a', 'b'])
  assert.deepEqual(ids(eligibleViewParents(views, 'a1x')), ['a', 'a1', 'a2', 'b'])
  assert.deepEqual(ids(eligibleViewParents(views, 'b')), ['a', 'a1', 'a2'])
})

test('reorderSlots permutes only the moved ids within their slots', () => {
  assert.deepEqual(reorderSlots(['a', 'a1', 'b', 'c'], ['c', 'a', 'b']), ['c', 'a1', 'a', 'b'])
})

const pipeline = (id: string, memberships: ViewMembership[]) => viewPipelineFixture({ id, memberships })

test('leaving the only sub-view that brought a pipeline into a parent also clears the parent', () => {
  const members = directMembers([
    pipeline('p', [
      { view: 'a', source: 'sub_view' },
      { view: 'a1', source: 'manual' },
    ]),
  ])

  assert.deepEqual([...viewMemberIds(views, members, 'a')], ['p'])

  const next = setDirectMembership(views, members, 'a1', ['p'], false)

  assert.deepEqual([...viewMemberIds(views, next, 'a1')], [])
  assert.deepEqual([...viewMemberIds(views, next, 'a')], [])
})

test('a parent keeps a pipeline after one sub-view drops it when it is still own or in a sibling', () => {
  const members = directMembers([
    pipeline('own', [
      { view: 'a', source: 'rule' },
      { view: 'a1', source: 'manual' },
    ]),
    pipeline('sibling', [
      { view: 'a', source: 'sub_view' },
      { view: 'a1', source: 'manual' },
      { view: 'a2', source: 'manual' },
    ]),
  ])

  const next = setDirectMembership(views, members, 'a1', ['own', 'sibling'], false)

  assert.deepEqual([...viewMemberIds(views, next, 'a')].sort(), ['own', 'sibling'])
  assert.deepEqual([...viewMemberIds(views, next, 'a1')], [])
})

test('removing a pipeline from a parent removes it from the whole subtree', () => {
  const members = directMembers([
    pipeline('p', [
      { view: 'a', source: 'sub_view' },
      { view: 'a1', source: 'sub_view' },
      { view: 'a1x', source: 'manual' },
      { view: 'b', source: 'manual' },
    ]),
  ])

  const next = setDirectMembership(views, members, 'a', ['p'], false)

  assert.deepEqual([...viewMemberIds(views, next, 'a1x')], [])
  assert.deepEqual([...viewMemberIds(views, next, 'b')], ['p'])
})
