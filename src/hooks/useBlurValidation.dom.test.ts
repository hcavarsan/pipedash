import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { useBlurValidation } = await import('./useBlurValidation')

afterEach(async () => {
  await act(async () => cleanup())
})
after(async () => {
  await GlobalRegistrator.unregister()
})

type Key = 'name' | 'code'

const validate = (key: Key, value: string): string | undefined => (value.trim() ? undefined : `${key} required`)

test('no error before blur, error after blur', () => {
  const { result } = renderHook(() => useBlurValidation<Key>(validate))

  assert.deepEqual(result.current.errors, {})

  act(() => result.current.onBlur('name', ''))

  assert.equal(result.current.errors.name, 'name required')
})

test('change revalidates only when the key already has an error', () => {
  const { result } = renderHook(() => useBlurValidation<Key>(validate))

  act(() => result.current.onChange('name', ''))
  assert.equal(result.current.errors.name, undefined)

  act(() => result.current.onBlur('name', ''))
  act(() => result.current.onChange('name', 'x'))
  assert.equal(result.current.errors.name, undefined)

  act(() => result.current.onBlur('name', ''))
  act(() => result.current.onChange('name', ' '))
  assert.equal(result.current.errors.name, 'name required')
})

test('validateAll stores every error and reports validity; reset clears them', () => {
  const { result } = renderHook(() => useBlurValidation<Key>(validate))
  let valid = true

  act(() => {
    valid = result.current.validateAll({ name: '', code: '' })
  })
  assert.equal(valid, false)
  assert.deepEqual(result.current.errors, { name: 'name required', code: 'code required' })

  act(() => result.current.reset())
  assert.deepEqual(result.current.errors, {})

  act(() => {
    valid = result.current.validateAll({ name: 'a', code: 'b' })
  })
  assert.equal(valid, true)
  assert.deepEqual(result.current.errors, {})
})
