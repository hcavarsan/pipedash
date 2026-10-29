import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 393, height: 852, settings: { device: { prefersReducedMotion: 'reduce' } } })
after(() => GlobalRegistrator.unregister())

const { cleanup, renderHook } = await import('@testing-library/react')
const { createSpringValue } = await import('./springValue')
const { useMobileRouteTransition } = await import('./useMobileRouteTransition')

afterEach(() => {
  cleanup()
  document.body.replaceChildren()
})

const settle = async () => {
  for (let turn = 0; turn < 4; turn += 1) {
    await Promise.resolve()
  }
}

const mount = () => {
  const main = document.createElement('main')
  const host = document.createElement('div')
  const page = document.createElement('div')
  const underlay = document.createElement('div')
  const overlay = document.createElement('div')
  const fades: Animation[] = []
  const calls = { clear: 0 }
  const animate = overlay.animate.bind(overlay)

  main.className = 'mantine-AppShell-main'
  main.append(host, page, underlay, overlay)
  document.body.append(main)
  overlay.animate = (keyframes, options) => {
    const fade = animate(keyframes, options)

    fades.push(fade)

    return fade
  }

  const motion = {
    motion: createSpringValue(0, () => undefined),
    mode: { current: 'idle' as const },
    width: { current: 393 },
    engage: () => undefined,
    clearLayers: () => {
      calls.clear += 1
    },
  }
  const layers = {
    host: { current: host },
    page: { current: page },
    underlay: { current: underlay },
    overlay: { current: overlay },
  }

  const view = renderHook(
    (pathname: string) =>
      useMobileRouteTransition({
        pathname,
        search: '',
        from: null,
        routeKey: pathname,
        layers,
        motion,
        scrollTop: { current: 0 },
        collapse: { current: 0 },
        swiped: { current: false },
        measure: () => undefined,
        arrive: () => undefined,
      }),
    { initialProps: '/pipelines/run-1' }
  )

  return { fades, calls, rerender: view.rerender, unmount: view.unmount }
}

test('a reduced-motion pop clears its layers when the fade finishes', async () => {
  const { fades, calls, rerender } = mount()

  rerender('/pipelines')
  const fade = fades[0]

  assert.ok(fade)
  const before = calls.clear

  fade.finish()
  await settle()

  assert.equal(calls.clear, before + 1)
})

test('unmounting cancels the running fade and a late finish clears nothing', async () => {
  const { fades, calls, rerender, unmount } = mount()

  rerender('/pipelines')
  const fade = fades[0]

  assert.ok(fade)
  assert.equal(fade.playState, 'running')
  unmount()
  const before = calls.clear

  assert.equal(fade.playState, 'idle')
  fade.finish()
  await settle()

  assert.equal(calls.clear, before)
})
