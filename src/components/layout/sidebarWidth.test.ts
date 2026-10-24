import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  nextSidebarDrag,
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  type SidebarDrag,
  type SidebarDragEvent,
  sidebarCssWidth,
  sizeAfterStep,
  sizeAtOffset,
} from './sidebarWidth'

const expanded = { width: 300, compact: false }
const rail = { width: 300, compact: true }
const wide = 1600

test('follows the pointer between the minimum and maximum width', () => {
  assert.deepEqual(sizeAtOffset(250.6, expanded, wide), { width: 251, compact: false })
  assert.deepEqual(sizeAtOffset(150, expanded, wide), { width: 200, compact: false })
  assert.deepEqual(sizeAtOffset(900, expanded, wide), { width: SIDEBAR_MAX_WIDTH, compact: false })
})

const run = (events: SidebarDragEvent[], resizable = true): SidebarDrag =>
  events.reduce(nextSidebarDrag, { resizable, draft: null })

test('a drag keeps a draft until it stops', () => {
  const dragging = run([
    { type: 'start', size: expanded },
    { type: 'move', offset: 350, viewportWidth: wide },
  ])

  assert.deepEqual(dragging.draft, { width: 350, compact: false })
  assert.equal(nextSidebarDrag(dragging, { type: 'stop' }).draft, null)
})

test('losing the handle mid-drag cancels the drag and ignores later pointer moves', () => {
  const hidden = run([
    { type: 'start', size: expanded },
    { type: 'move', offset: 350, viewportWidth: wide },
    { type: 'resizable', resizable: false },
    { type: 'move', offset: 380, viewportWidth: wide },
  ])
  assert.deepEqual(hidden, { resizable: false, draft: null })

  assert.deepEqual(nextSidebarDrag(hidden, { type: 'resizable', resizable: true }), { resizable: true, draft: null })
})

test('a drag cannot start while the sidebar is not resizable', () => {
  assert.deepEqual(run([{ type: 'start', size: expanded }], false), { resizable: false, draft: null })
})

test('snaps to the rail below the midpoint and keeps the expanded width to restore', () => {
  assert.deepEqual(sizeAtOffset(133, expanded, wide), { width: 300, compact: true })
  assert.deepEqual(sizeAtOffset(134, rail, wide), { width: 200, compact: false })
})

test('caps the width at 40% of a narrow window', () => {
  assert.deepEqual(sizeAtOffset(400, expanded, 800), { width: 320, compact: false })
  assert.deepEqual(sizeAtOffset(400, expanded, 300), { width: 200, compact: false })
})

test('keyboard steps shrink into the rail and grow back out of it', () => {
  assert.deepEqual(sizeAfterStep(-16, { width: 210, compact: false }, wide), { width: 210, compact: true })
  assert.deepEqual(sizeAfterStep(-16, rail, wide), rail)
  assert.deepEqual(sizeAfterStep(16, rail, wide), { width: 300, compact: false })
  assert.deepEqual(sizeAfterStep(16, { width: SIDEBAR_MAX_WIDTH, compact: false }, wide), {
    width: SIDEBAR_MAX_WIDTH,
    compact: false,
  })
  assert.deepEqual(sizeAfterStep(-16, { width: SIDEBAR_MAX_WIDTH, compact: false }, 900), {
    width: 344,
    compact: false,
  })
})

test('clamps a stored width that is out of range or not a number', () => {
  assert.equal(sidebarCssWidth(300), 'clamp(200px, 300px, 40vw)')
  assert.equal(sidebarCssWidth(5000), `clamp(200px, ${SIDEBAR_MAX_WIDTH}px, 40vw)`)
  assert.equal(sidebarCssWidth(Number.NaN), `clamp(200px, ${SIDEBAR_DEFAULT_WIDTH}px, 40vw)`)
})
