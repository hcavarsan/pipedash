import assert from 'node:assert/strict'
import { describe, it, mock } from 'node:test'

import { createLongPress, guardLongPressClick, LONG_PRESS_MS } from './longPress'

const harness = () => {
  let pending: { callback: () => void; ms: number } | null = null
  let fires = 0
  const press = createLongPress(
    () => {
      fires += 1
    },
    {
      schedule: (callback, ms) => {
        pending = { callback, ms }

        return setTimeout(() => undefined, 0)
      },
      unschedule: () => {
        pending = null
      },
    }
  )

  return {
    press,
    fires: () => fires,
    elapse: () => {
      const current = pending

      pending = null
      current?.callback()
    },
    scheduledMs: () => pending?.ms ?? null,
  }
}

describe('createLongPress', () => {
  it('fires after the delay and reports it on release', () => {
    const h = harness()

    h.press.start(10, 10)
    assert.equal(h.scheduledMs(), LONG_PRESS_MS)
    h.elapse()
    assert.equal(h.fires(), 1)
    assert.equal(h.press.end(), true)
    assert.equal(h.press.end(), false)
  })

  it('tolerates movement within the slop', () => {
    const h = harness()

    h.press.start(0, 0)
    h.press.move(6, 7)
    assert.notEqual(h.scheduledMs(), null)
    h.elapse()
    assert.equal(h.fires(), 1)
  })

  it('cancels when the finger moves past the slop', () => {
    const h = harness()

    h.press.start(0, 0)
    h.press.move(11, 0)
    assert.equal(h.scheduledMs(), null)
    h.elapse()
    assert.equal(h.fires(), 0)
    assert.equal(h.press.end(), false)
  })

  it('cancels on release before the delay', () => {
    const h = harness()

    h.press.start(0, 0)
    assert.equal(h.press.end(), false)
    assert.equal(h.scheduledMs(), null)
    assert.equal(h.fires(), 0)
  })

  it('cancels explicitly (scroll)', () => {
    const h = harness()

    h.press.start(0, 0)
    h.press.cancel()
    h.elapse()
    assert.equal(h.fires(), 0)
  })

  it('restarts cleanly on a second press', () => {
    const h = harness()

    h.press.start(0, 0)
    h.elapse()
    h.press.end()
    h.press.start(5, 5)
    assert.equal(h.press.end(), false)
    assert.equal(h.fires(), 1)
  })
})

describe('guardLongPressClick', () => {
  it('blocks the releasing pointer even when its click targets the menu overlay', () => {
    const overlay = new EventTarget()
    const remove = guardLongPressClick(overlay, 2)
    const release = Object.assign(new Event('click', { cancelable: true }), { pointerId: 2 })
    const mouseDown = new Event('mousedown', { cancelable: true })

    overlay.dispatchEvent(mouseDown)

    assert.equal(mouseDown.defaultPrevented, true)

    overlay.dispatchEvent(release)

    assert.equal(release.defaultPrevented, true)
    remove()
  })

  it('allows a new pointer to select a menu item immediately', () => {
    const menu = new EventTarget()
    const remove = guardLongPressClick(menu, 2)
    const nextTap = Object.assign(new Event('click', { cancelable: true }), { pointerId: 3 })
    const pointerDown = Object.assign(new Event('pointerdown'), { pointerId: 3 })
    const mouseDown = new Event('mousedown', { cancelable: true })

    menu.dispatchEvent(pointerDown)
    menu.dispatchEvent(mouseDown)

    menu.dispatchEvent(nextTap)

    assert.equal(nextTap.defaultPrevented, false)
    assert.equal(mouseDown.defaultPrevented, false)
    remove()
  })

  it('drops the guard after 600ms so the next click goes through', () => {
    mock.timers.enable({ apis: ['setTimeout'] })
    const target = new EventTarget()
    const remove = guardLongPressClick(target, 2)
    const early = new Event('mousedown', { cancelable: true })

    mock.timers.tick(599)
    target.dispatchEvent(early)
    assert.equal(early.defaultPrevented, true)

    mock.timers.tick(1)

    const click = Object.assign(new Event('click', { cancelable: true }), { pointerId: 2 })

    target.dispatchEvent(click)
    assert.equal(click.defaultPrevented, false)
    remove()
    mock.timers.reset()
  })

  it('disposes idempotently', () => {
    const target = new EventTarget()
    const remove = guardLongPressClick(target, 2)
    const click = Object.assign(new Event('click', { cancelable: true }), { pointerId: 2 })

    remove()
    remove()
    target.dispatchEvent(click)
    assert.equal(click.defaultPrevented, false)
  })
})
