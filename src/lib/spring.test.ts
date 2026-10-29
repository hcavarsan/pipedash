import assert from 'node:assert/strict'
import { mock, test } from 'node:test'

import {
  animateSpring,
  project,
  rubberband,
  SPRING_DEFAULT,
  SPRING_MOMENTUM,
  type SpringConfig,
  springState,
} from './spring'

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

test('a stopped animation keeps reporting the position it stopped at', () => {
  let now = 0
  const frames: Array<(time: number) => void> = []
  mock.method(performance, 'now', () => now)
  Object.assign(globalThis, {
    requestAnimationFrame: (callback: (time: number) => void) => frames.push(callback),
    cancelAnimationFrame: () => undefined,
  })

  try {
    const updates: number[] = []
    const animation = animateSpring({ from: 0, to: 100, onUpdate: (value) => updates.push(value) })

    now = 80
    frames.shift()?.(now)
    animation.stop()
    const stopped = animation.value
    now = 5000

    assert.ok(stopped > 0 && stopped < 100)
    assert.equal(animation.value, stopped)
    assert.equal(animation.velocity, 0)
  } finally {
    mock.restoreAll()
    Reflect.deleteProperty(globalThis, 'requestAnimationFrame')
    Reflect.deleteProperty(globalThis, 'cancelAnimationFrame')
  }
})

test('a completed animation reports its target at rest', () => {
  let now = 0
  const frames: Array<(time: number) => void> = []
  mock.method(performance, 'now', () => now)
  Object.assign(globalThis, {
    requestAnimationFrame: (callback: (time: number) => void) => frames.push(callback),
    cancelAnimationFrame: () => undefined,
  })

  try {
    const animation = animateSpring({ from: 0, to: 100, onUpdate: () => undefined })

    now = 10_000
    frames.shift()?.(now)

    assert.equal(animation.value, 100)
    assert.equal(animation.velocity, 0)
  } finally {
    mock.restoreAll()
    Reflect.deleteProperty(globalThis, 'requestAnimationFrame')
    Reflect.deleteProperty(globalThis, 'cancelAnimationFrame')
  }
})
