import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 393, height: 852, settings: { device: { prefersReducedMotion: 'reduce' } } })
after(() => GlobalRegistrator.unregister())

const { cleanup, renderHook } = await import('@testing-library/react')
const { createSpringValue } = await import('./springValue')
const { useMobileRouteTransition } = await import('./useMobileRouteTransition')
const { BROWSE_PATH, groupPath, pushState, splitLocation, providerPath } = await import('./mobileChrome')

afterEach(() => {
  cleanup()
  document.body.replaceChildren()
})

const settle = async () => {
  for (let turn = 0; turn < 4; turn += 1) {
    await Promise.resolve()
  }
}

interface Location {
  pathname: string
  state?: unknown
}

const mount = (initial: Location = { pathname: '/pipelines/run-1', state: pushState('/pipelines', '') }) => {
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
  const animatePage = page.animate.bind(page)
  page.animate = (keyframes, options) => {
    const fade = animatePage(keyframes, options)

    fade.finished.catch(() => undefined)

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
  const scrollTop = { current: 0 }

  const view = renderHook(
    ({ pathname, state }: Location) =>
      useMobileRouteTransition({
        pathname,
        search: '',
        state,
        routeKey: pathname,
        layers,
        motion,
        scrollTop,
        collapse: { current: 0 },
        swiped: { current: false },
        measure: () => undefined,
        arrive: () => undefined,
      }),
    { initialProps: initial }
  )

  return {
    main,
    fades,
    calls,
    scrollTop,
    go: (pathname: string, state?: unknown) => view.rerender({ pathname, state }),
    unmount: view.unmount,
  }
}

test('a reduced-motion pop clears its layers when the fade finishes', async () => {
  const { fades, calls, go } = mount()

  go('/pipelines')
  const fade = fades[0]

  assert.ok(fade)
  const before = calls.clear

  fade.finish()
  await settle()

  assert.equal(calls.clear, before + 1)
})

test('unmounting cancels the running fade and a late finish clears nothing', async () => {
  const { fades, calls, go, unmount } = mount()

  go('/pipelines')
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

test('popping from a nested history back to its repository animates the pop and restores the scroll', () => {
  const provider = splitLocation(providerPath(1))
  const providerState = pushState(BROWSE_PATH, '')
  const repo = groupPath('repo:acme/mobile')
  const repoState = pushState(provider.pathname, provider.search, providerState)
  const historyState = pushState(repo, '', repoState)
  const { main, fades, scrollTop, go } = mount({ pathname: repo, state: repoState })

  Object.defineProperty(main, 'scrollHeight', { value: 2000 })
  Object.defineProperty(main, 'clientHeight', { value: 800 })
  scrollTop.current = 250
  go('/pipelines/abc', historyState)
  assert.equal(main.scrollTop, 0)
  assert.equal(fades.length, 0)

  go(repo, repoState)
  assert.equal(fades.length, 1)
  assert.equal(main.scrollTop, 250)
})
