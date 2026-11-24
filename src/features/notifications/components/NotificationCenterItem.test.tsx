import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { CenterNotification } from '../../../types/notifications'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { NotificationCenterItem } = await import('./NotificationCenterItem')

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

const stuckQueue: CenterNotification = {
  key: 'pipeline:p1',
  at: 0,
  read: false,
  tone: 'warning',
  label: 'Stuck in the queue',
  level: null,
  title: 'deploy is waiting in the queue too long',
  body: 'Stub',
  providerIds: [1],
  pipelineId: 'p1',
  link: null,
}

const renderItem = (item: CenterNotification) =>
  render(
    createElement(
      MantineProvider,
      null,
      createElement(NotificationCenterItem, { item, now: 0, onSelect: () => undefined })
    )
  )

test('a stuck queue alert shows the label the backend sent, not a long run label', () => {
  renderItem(stuckQueue)

  const view = within(document.body)
  assert.ok(view.getByText(/^Stuck in the queue · /))
  assert.equal(view.queryByText(/Running longer than usual/), null)
})

test('a notice from a server that sends no label shows only its time', () => {
  renderItem({ ...stuckQueue, label: undefined })

  assert.equal(within(document.body).queryByText(/undefined/), null)
})
