import assert from 'node:assert/strict'
import { test } from 'node:test'

import { labelsFit } from './alertFit'

test('labels that fit side by side keep the row layout', () => {
  assert.equal(
    labelsFit([
      { scrollWidth: 60, clientWidth: 100 },
      { scrollWidth: 100, clientWidth: 100 },
    ]),
    true
  )
})

test('subpixel rounding does not stack the buttons', () => {
  assert.equal(labelsFit([{ scrollWidth: 101, clientWidth: 100 }]), true)
})

test('any clipped label stacks the buttons regardless of character count', () => {
  assert.equal(
    labelsFit([
      { scrollWidth: 60, clientWidth: 100 },
      { scrollWidth: 140, clientWidth: 100 },
    ]),
    false
  )
})
