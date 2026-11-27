import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { RunLogNode } from '../../../types/logs'
import type { LogNodeGroup } from '../lib/logTree'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })

const layout = {
  clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight'),
  offsetHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight'),
}
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 900 })
Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
  configurable: true,
  get(this: HTMLElement) {
    return this.hasAttribute('data-virtual-item') ? 52 : 0
  },
})

const { act, cleanup, render, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { MemoryRouter } = await import('react-router-dom')
const { MobileRunJobs } = await import('./MobileRunJobs')
const view = () => within(document.body)

const job = (id: string, status: RunLogNode['status']): RunLogNode => ({
  id,
  parent_id: null,
  kind: 'job',
  name: id,
  status,
  started_at: null,
  finished_at: null,
  duration_seconds: null,
  log: 'ready',
  web_url: null,
  depth: 0,
  navigable: true,
})

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  for (const [name, descriptor] of Object.entries(layout)) {
    if (descriptor) {
      Object.defineProperty(HTMLElement.prototype, name, descriptor)
    }
  }
  await GlobalRegistrator.unregister()
})

const renderJobs = (groups: LogNodeGroup[]) =>
  render(
    createElement(
      MantineProvider,
      null,
      createElement(MemoryRouter, null, createElement(MobileRunJobs, { groups, onSelect: () => undefined }))
    )
  )

test('skipped jobs fold under one row with their count and a tap lists them', async () => {
  const user = userEvent.setup({ document })
  renderJobs([
    { stage: 'build', nodes: [job('compile', 'failed')], skipped: [job('lint', 'skipped'), job('docs', 'skipped')] },
  ])

  const fold = await view().findByRole('button', { name: /2 skipped jobs/ })
  assert.equal(fold.getAttribute('aria-expanded'), 'false')
  assert.ok(view().getByRole('button', { name: /compile/ }))
  assert.equal(view().queryByRole('button', { name: /lint/ }), null)

  await user.click(fold)

  assert.equal(fold.getAttribute('aria-expanded'), 'true')
  assert.ok(await view().findByRole('button', { name: /lint/ }))
  assert.ok(view().getByRole('button', { name: /docs/ }))
})

test('a run with no skipped jobs shows no fold row', async () => {
  renderJobs([{ stage: null, nodes: [job('compile', 'success')], skipped: [] }])

  assert.ok(await view().findByRole('button', { name: /compile/ }))
  assert.equal(view().queryByRole('button', { name: /skipped/ }), null)
})
