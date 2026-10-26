import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { registerScrollToTop, scrollActiveToTop } from './scrollToTop'

GlobalRegistrator.register({ width: 393, height: 852 })
after(() => GlobalRegistrator.unregister())

function scroller(covered: boolean) {
  const inner = document.createElement('div')

  if (covered) {
    inner.setAttribute('data-sheet-covered', '1')
  }

  const element = document.createElement('div')

  inner.append(element)
  document.body.append(inner)

  return { element, inner }
}

const registerFor = (element: HTMLElement, calls: string[], name: string) =>
  registerScrollToTop({
    isActive: () => !element.closest('[data-sheet-covered]'),
    scrollToTop: () => calls.push(name),
  })

test('the newest uncovered entry handles the tap and covered entries are skipped', () => {
  const calls: string[] = []
  const lower = scroller(false)
  const upper = scroller(false)
  const stopLower = registerFor(lower.element, calls, 'lower')
  const stopUpper = registerFor(upper.element, calls, 'upper')

  scrollActiveToTop()
  assert.deepEqual(calls, ['upper'])

  upper.inner.setAttribute('data-sheet-covered', '1')
  scrollActiveToTop()
  assert.deepEqual(calls, ['upper', 'lower'])

  stopUpper()
  stopLower()
})

test('cleanup removes exactly its own entry', () => {
  const calls: string[] = []
  const first = scroller(false)
  const second = scroller(false)
  const stopFirst = registerFor(first.element, calls, 'first')
  const stopSecond = registerFor(second.element, calls, 'second')

  stopFirst()
  stopFirst()
  scrollActiveToTop()
  assert.deepEqual(calls, ['second'])

  stopSecond()
  scrollActiveToTop()
  assert.deepEqual(calls, ['second'])
})

test('with no active entry the app shell scrolls to the top', () => {
  const main = document.createElement('main')
  const scrolls: unknown[] = []

  main.className = 'mantine-AppShell-main'
  main.scrollTo = ((options: unknown) => scrolls.push(options)) as typeof main.scrollTo
  document.body.append(main)

  const hidden = scroller(true)
  const stop = registerFor(hidden.element, [], 'hidden')

  scrollActiveToTop()
  assert.equal(scrolls.length, 1)
  assert.equal((scrolls[0] as ScrollToOptions).top, 0)

  stop()
})
