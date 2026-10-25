import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

import { DISARM_HYSTERESIS, nextArmed, settleDecision } from './swipeArmed'

const WIDTH = 400
const THRESHOLD = WIDTH * 0.6

describe('nextArmed', () => {
  it('arms when crossing the threshold', () => {
    assert.equal(nextArmed(false, THRESHOLD - 1, WIDTH), false)
    assert.equal(nextArmed(false, THRESHOLD, WIDTH), true)
  })

  it('stays armed inside the hysteresis band and disarms after a deliberate retreat', () => {
    assert.equal(nextArmed(true, THRESHOLD - DISARM_HYSTERESIS + 1, WIDTH), true)
    assert.equal(nextArmed(true, THRESHOLD - DISARM_HYSTERESIS - 1, WIDTH), false)
  })

  it('does not re-arm inside the band once disarmed', () => {
    assert.equal(nextArmed(false, THRESHOLD - 5, WIDTH), false)
  })
})

describe('settleDecision', () => {
  it('promotes open to full only while armed', () => {
    assert.equal(settleDecision('open', true), 'full')
    assert.equal(settleDecision('open', false), 'open')
  })

  it('keeps a reversal closed even when armed', () => {
    assert.equal(settleDecision('closed', true), 'closed')
  })
})
