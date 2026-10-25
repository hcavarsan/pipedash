import assert from 'node:assert/strict'
import { test } from 'node:test'

import { project, rubberband, SPRING_DEFAULT, SPRING_MOMENTUM, type SpringConfig, springState } from './spring'

const CONFIGS: [string, SpringConfig][] = [
  ['critically damped', SPRING_DEFAULT],
  ['under damped', SPRING_MOMENTUM],
  ['over damped', { damping: 1.4, response: 0.35 }],
]

for (const [name, config] of CONFIGS) {
  test(`${name} spring starts at its displacement and velocity and settles at the target`, () => {
    const start = springState(-200, 1500, 0, config)
    const settled = springState(-200, 1500, 3, config)

    assert.ok(Math.abs(start.displacement + 200) < 1e-6)
    assert.ok(Math.abs(start.velocity - 1500) < 1e-6)
    assert.ok(Math.abs(settled.displacement) < 0.5)
    assert.ok(Math.abs(settled.velocity) < 10)
  })
}

test('only the under damped spring overshoots the target', () => {
  const peak = (config: SpringConfig) =>
    Math.max(...Array.from({ length: 300 }, (_, frame) => springState(-100, 0, frame / 120, config).displacement))

  assert.ok(peak(SPRING_DEFAULT) <= 0)
  assert.ok(peak(SPRING_MOMENTUM) > 0)
})

test('project throws a flick forward with the scroll deceleration rate', () => {
  assert.ok(Math.abs(project(100, 1000) - (100 + 0.998 / 0.002)) < 1e-6)
  assert.ok(Math.abs(project(100, -1000) - (100 - 0.998 / 0.002)) < 1e-6)
  assert.equal(project(100, 0), 100)
})

test('rubberband keeps the sign and never reaches the dimension', () => {
  assert.equal(rubberband(0, 300), 0)
  assert.ok(rubberband(-50, 300) < 0)
  assert.ok(rubberband(10_000, 300) < 300)
  assert.ok(rubberband(50, 300) < 50)
})
