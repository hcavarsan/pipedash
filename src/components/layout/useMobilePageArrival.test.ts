import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 393, height: 852 })
after(() => GlobalRegistrator.unregister())

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { useMobilePageArrival } = await import('./useMobilePageArrival')

const nativeRequest = globalThis.requestAnimationFrame
const nativeCancel = globalThis.cancelAnimationFrame
let frames = new Map<number, () => void>()
let nextFrame = 1

const flushFrames = () => {
  const pending = [...frames.values()]

  frames = new Map()
  for (const run of pending) {
    run()
  }
}

beforeEach(() => {
  frames = new Map()
  nextFrame = 1
  globalThis.requestAnimationFrame = (callback) => {
    const id = nextFrame++

    frames.set(id, () => callback(0))

    return id
  }
  globalThis.cancelAnimationFrame = (id) => {
    frames.delete(id)
  }
})

afterEach(() => {
  cleanup()
  globalThis.requestAnimationFrame = nativeRequest
  globalThis.cancelAnimationFrame = nativeCancel
})

const mount = (title: string) => {
  const page = document.createElement('div')
  const region = document.createElement('div')
  const { result, rerender, unmount } = renderHook(
    (current: string) => useMobilePageArrival({ current: page }, { current: region }, current),
    { initialProps: title }
  )

  return { arrive: () => result.current(), region, rerender, unmount }
}

test('an arrival announces the current title on the next frame', () => {
  const { arrive, region } = mount('History')

  act(() => arrive())
  assert.equal(region.textContent, '')
  act(() => flushFrames())
  assert.equal(region.textContent, 'History')
})

test('a newer arrival replaces the pending announcement', () => {
  const { arrive, region, rerender } = mount('History')

  act(() => arrive())
  rerender('Pipelines')
  act(() => arrive())

  assert.equal(frames.size, 1)
  act(() => flushFrames())
  assert.equal(region.textContent, 'Pipelines')
})

test('a pending announcement never lands after the page unmounts', () => {
  const { arrive, region, unmount } = mount('History')

  act(() => arrive())
  unmount()
  act(() => flushFrames())

  assert.equal(region.textContent, '')
})
