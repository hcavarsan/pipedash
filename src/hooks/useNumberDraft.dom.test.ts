import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { useNumberDraft } = await import('./useNumberDraft')

afterEach(async () => {
  await act(async () => cleanup())
})
after(async () => {
  await GlobalRegistrator.unregister()
})

test('a blank commit sets the error and does not save', () => {
  const saves: number[] = []
  const { result } = renderHook(() =>
    useNumberDraft({ saved: 7, min: 1, save: async (value) => void saves.push(value) })
  )

  act(() => result.current.onChange(''))
  act(() => result.current.commit())

  assert.equal(result.current.error, 'Enter a number')
  assert.deepEqual(saves, [])
})

test('a value below the minimum or not an integer does not save', () => {
  const saves: number[] = []
  const { result } = renderHook(() =>
    useNumberDraft({ saved: 7, min: 1, save: async (value) => void saves.push(value) })
  )

  act(() => result.current.onChange(0))
  act(() => result.current.commit())
  assert.equal(result.current.error, 'Must be at least 1')

  act(() => result.current.onChange(2.5))
  act(() => result.current.commit())
  assert.equal(result.current.error, 'Must be at least 1')
  assert.deepEqual(saves, [])
})

test('a rejected save restores the saved value and sets the error', async () => {
  const { result } = renderHook(() =>
    useNumberDraft({
      saved: 7,
      min: 1,
      save: async () => {
        throw new Error('nope')
      },
    })
  )

  act(() => result.current.onChange(12))
  await act(async () => result.current.commit())

  assert.equal(result.current.value, 7)
  assert.equal(result.current.error, 'nope')
  assert.equal(result.current.pending, false)
})

test('an unchanged value is a no-op and a second commit while pending is ignored', async () => {
  const saves: number[] = []
  const gate = Promise.withResolvers<void>()
  const { result } = renderHook(() =>
    useNumberDraft({
      saved: 7,
      min: 1,
      save: async (value) => {
        saves.push(value)
        await gate.promise
      },
    })
  )

  act(() => result.current.onChange('7'))
  act(() => result.current.commit())
  assert.deepEqual(saves, [])

  act(() => result.current.onChange(9))
  act(() => {
    result.current.commit()
    result.current.commit()
  })
  assert.deepEqual(saves, [9])
  assert.equal(result.current.pending, true)

  await act(async () => gate.resolve())
  assert.equal(result.current.pending, false)
})
