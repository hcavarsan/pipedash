import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  edgeOffset,
  layoutBar,
  resolveBack,
  scrollEdge,
  swipeProgress,
  tabAction,
  tabBarScroll,
  titleCollapse,
  transitionKind,
} from './navigation'

test('back pops only when the previous entry is the parent', () => {
  assert.equal(resolveBack(2, '/pipelines?view=pinned', '/pipelines?view=pinned'), 'pop')
  assert.equal(resolveBack(2, '/providers', '/pipelines?view=pinned'), 'replace')
  assert.equal(resolveBack(1, undefined, '/pipelines'), 'replace')
  assert.equal(resolveBack(0, '/pipelines', '/pipelines'), 'replace')
})

test('transitions follow depth inside one tab and never cross tabs', () => {
  assert.equal(transitionKind({ tab: 'pipelines', depth: 0 }, { tab: 'pipelines', depth: 1 }), 'push')
  assert.equal(transitionKind({ tab: 'notifications', depth: 1 }, { tab: 'notifications', depth: 0 }), 'pop')
  assert.equal(transitionKind({ tab: 'pipelines', depth: 0 }, { tab: 'providers', depth: 0 }), 'none')
  assert.equal(transitionKind({ tab: 'pipelines', depth: 1 }, { tab: 'settings', depth: 1 }), 'none')
  assert.equal(transitionKind({ tab: 'settings', depth: 0 }, { tab: 'settings', depth: 0 }), 'none')
})

test('tab presses switch, pop to root, or scroll to top', () => {
  assert.equal(tabAction('pipelines', 'providers', false), 'switch')
  assert.equal(tabAction('pipelines', 'providers', true), 'switch')
  assert.equal(tabAction('pipelines', 'pipelines', true), 'pop-to-root')
  assert.equal(tabAction('pipelines', 'pipelines', false), 'scroll-top')
})

test('title collapse is continuous between the bar and half a title height', () => {
  assert.equal(titleCollapse(200, 88, 40), 0)
  assert.equal(titleCollapse(108, 88, 40), 0)
  assert.equal(titleCollapse(98, 88, 40), 0.5)
  assert.equal(titleCollapse(88, 88, 40), 1)
  assert.equal(titleCollapse(40, 88, 40), 1)
})

test('scroll edge fades in over the first pixels of overlap', () => {
  assert.equal(scrollEdge(0), 0)
  assert.equal(scrollEdge(8), 0.5)
  assert.equal(scrollEdge(400), 1)
  assert.equal(scrollEdge(-20), 0)
})

test('edge offset rubber-bands past both bounds', () => {
  assert.equal(edgeOffset(120, 390), 120)
  assert.ok(edgeOffset(-100, 390) < 0 && edgeOffset(-100, 390) > -100)
  assert.ok(edgeOffset(490, 390) > 390 && edgeOffset(490, 390) < 490)
})

test('swipe progress is clamped', () => {
  assert.equal(swipeProgress(-10, 390), 0)
  assert.equal(swipeProgress(195, 390), 0.5)
  assert.equal(swipeProgress(500, 390), 1)
  assert.equal(swipeProgress(10, 0), 0)
})

test('the back label shrinks, then falls back to Back', () => {
  const base = { bar: 375, leading: 34, short: 38, trailing: 44 }

  assert.deepEqual(layoutBar({ ...base, label: 70 }), { label: 'full', labelMax: 70, side: 104 })
  assert.deepEqual(layoutBar({ ...base, label: 200 }), { label: 'full', labelMax: 105.5, side: 139.5 })
  assert.deepEqual(layoutBar({ ...base, bar: 200, label: 200 }), { label: 'short', labelMax: 38, side: 72 })
  assert.deepEqual(layoutBar({ ...base, label: 0 }), { label: 'full', labelMax: 0, side: 44 })
})

test('trailing items widen the protected side', () => {
  assert.equal(layoutBar({ bar: 375, leading: 34, label: 70, short: 38, trailing: 150 }).side, 150)
})

test('the tab bar minimizes after scrolling down and expands after scrolling up or at the top', () => {
  const steps: Array<[number, { minimized: boolean; anchor: number } | 'same']> = [
    [5, 'same'],
    [20, { minimized: true, anchor: 20 }],
    [50, { minimized: true, anchor: 50 }],
    [45, 'same'],
    [30, { minimized: false, anchor: 30 }],
    [0, { minimized: false, anchor: 0 }],
  ]
  let state = { minimized: false, anchor: 0 }

  for (const [top, expected] of steps) {
    const next = tabBarScroll(state, top)

    assert.deepEqual(next, expected === 'same' ? state : expected, `top ${top}`)
    state = next
  }
})
