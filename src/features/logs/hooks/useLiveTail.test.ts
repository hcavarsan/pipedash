import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { useLiveTail } = await import('./useLiveTail')

afterEach(() => cleanup())

after(async () => {
  await GlobalRegistrator.unregister()
})

const setup = () =>
  renderHook(({ revision, lines }) => useLiveTail(revision, lines, lines), {
    initialProps: { revision: 1, lines: 100 },
  })

test('new lines are counted while the viewer is away from the end and cleared once it is back', () => {
  const { result, rerender } = setup()

  act(() => result.current.onViewport(99))
  rerender({ revision: 2, lines: 110 })
  assert.equal(result.current.behind, 0)

  act(() => result.current.onViewport(50))
  rerender({ revision: 3, lines: 125 })
  assert.equal(result.current.behind, 15)

  rerender({ revision: 4, lines: 130 })
  assert.equal(result.current.behind, 20)

  act(() => result.current.onViewport(129))
  assert.equal(result.current.behind, 0)
})

test('a log opened away from the end reports nothing new until more lines arrive', () => {
  const { result, rerender } = setup()

  act(() => result.current.onViewport(10))
  assert.equal(result.current.behind, 0)

  rerender({ revision: 2, lines: 101 })
  assert.equal(result.current.behind, 1)
})

test('freshly arrived lines are flagged from the previous end and only briefly', (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const { result, rerender } = setup()

  assert.equal(result.current.freshFrom, null)

  rerender({ revision: 2, lines: 110 })
  assert.equal(result.current.freshFrom, 100)

  rerender({ revision: 3, lines: 110 })
  assert.equal(result.current.freshFrom, 100)

  act(() => context.mock.timers.tick(1500))
  assert.equal(result.current.freshFrom, null)
})
