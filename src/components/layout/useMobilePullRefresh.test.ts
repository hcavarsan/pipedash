import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 393, height: 852 })
after(() => GlobalRegistrator.unregister())

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { useMobilePullRefresh } = await import('./useMobilePullRefresh')

const nativeSetTimeout = window.setTimeout
let delays: number[] = []

beforeEach(() => {
  delays = []
  window.setTimeout = Object.assign((handler: TimerHandler, delay?: number, ...args: unknown[]) => {
    delays.push(delay ?? 0)

    return nativeSetTimeout(handler, delay, ...args)
  }, nativeSetTimeout)
})

afterEach(() => {
  cleanup()
  window.setTimeout = nativeSetTimeout
  document.body.replaceChildren()
})

const mount = (refresh: () => Promise<unknown>) => {
  const main = document.createElement('main')
  const host = document.createElement('div')
  const content = document.createElement('div')
  const spinner = document.createElement('div')
  const status = document.createElement('div')

  main.className = 'mantine-AppShell-main'
  Object.defineProperty(main, 'clientHeight', { value: 800 })
  main.hasPointerCapture = () => true
  main.setPointerCapture = () => undefined
  main.append(host, content)
  document.body.append(main, spinner, status)

  const view = renderHook(() =>
    useMobilePullRefresh({
      host: { current: host },
      content: { current: content },
      spinner: { current: spinner },
      status: { current: status },
      refresh,
    })
  )

  return { main, spinner, status, unmount: view.unmount }
}

const pullAndRelease = (main: HTMLElement) => {
  const init = { pointerId: 1, pointerType: 'mouse', button: 0, bubbles: true, cancelable: true }

  act(() => {
    main.dispatchEvent(new PointerEvent('pointerdown', { ...init, clientY: 0 }))
    main.dispatchEvent(new PointerEvent('pointermove', { ...init, clientY: 600 }))
    main.dispatchEvent(new PointerEvent('pointerup', { ...init, clientY: 600 }))
  })
}

const deferred = () => Promise.withResolvers<void>()

test('a pull past the threshold refreshes and reports the result', async () => {
  const pending = deferred()
  const { main, spinner, status } = mount(() => pending.promise)

  pullAndRelease(main)
  assert.equal(status.textContent, 'Refreshing…')
  assert.ok(spinner.hasAttribute('data-spinning'))
  assert.equal(main.getAttribute('aria-busy'), 'true')

  await act(async () => pending.resolve())

  assert.equal(status.textContent, 'Updated')
  assert.ok(!spinner.hasAttribute('data-spinning'))
  assert.ok(!main.hasAttribute('aria-busy'))
  assert.ok(delays.includes(2000))
})

test('a refresh that finishes after the page unmounted writes no status and starts no timer', async () => {
  const pending = deferred()
  const { main, status, unmount } = mount(() => pending.promise)

  pullAndRelease(main)
  assert.equal(status.textContent, 'Refreshing…')

  unmount()
  delays = []
  await act(async () => pending.resolve())

  assert.equal(status.textContent, 'Refreshing…')
  assert.deepEqual(delays, [])
})

test('a refresh that fails after the page unmounted writes no status', async () => {
  const pending = deferred()
  const { main, status, unmount } = mount(() => pending.promise)

  pullAndRelease(main)
  unmount()
  delays = []
  await act(async () => pending.reject())

  assert.equal(status.textContent, 'Refreshing…')
  assert.deepEqual(delays, [])
})
