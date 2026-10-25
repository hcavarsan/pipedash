import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { SPRING_DEFAULT } from '../../lib/spring'
import { createSearchReveal } from './searchReveal'

GlobalRegistrator.register()
after(() => GlobalRegistrator.unregister())

window.matchMedia = ((query: string) => ({ matches: query.includes('reduce') })) as typeof window.matchMedia

const mount = () => {
  const root = document.createElement('div')
  const element = document.createElement('div')
  const bar = document.createElement('div')

  Object.defineProperty(bar, 'offsetHeight', { value: 60 })
  element.setAttribute('data-search-reveal', '')
  element.append(bar)
  root.append(element)

  return { root, element }
}

test('a row hidden after opening reveals again from zero', async () => {
  const { root, element } = mount()
  const reveals = createSearchReveal(() => root)
  const first = reveals.closed()

  assert.ok(first)
  first.spring.set(40)
  await reveals.settle(first, true, 0, SPRING_DEFAULT)
  assert.ok(element.hasAttribute('data-open'))

  element.removeAttribute('data-open')

  assert.equal(reveals.closed()?.spring.value, 0)
})

test('a hidden tab does not intercept the visible search bar', () => {
  const { root, element } = mount()
  const hidden = document.createElement('div')
  const bar = document.createElement('div')

  hidden.setAttribute('data-search-reveal', '')
  hidden.append(bar)
  root.prepend(hidden)

  assert.equal(createSearchReveal(() => root).closed()?.element, element)
})

test('search height is measured again after its layout changes', () => {
  const { root, element } = mount()
  const bar = document.createElement('div')
  const reveals = createSearchReveal(() => root)

  assert.equal(reveals.closed()?.full, 60)
  Object.defineProperty(bar, 'offsetHeight', { value: 80 })
  element.replaceChildren(bar)

  assert.equal(reveals.closed()?.full, 80)
})
