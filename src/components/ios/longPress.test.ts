import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

import { createLongPress, LONG_PRESS_MS } from './longPress'

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

        return 1 as unknown as ReturnType<typeof setTimeout>
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
