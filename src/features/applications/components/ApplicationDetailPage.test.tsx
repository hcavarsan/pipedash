import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { detailFixture } from './applicationFixtures'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { ApplicationDetailPage } = await import('./ApplicationDetailPage')

const client = new QueryClient()

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(
    MantineProvider,
    { env: 'test' },
    createElement(QueryClientProvider, { client }, createElement(MemoryRouter, null, children))
  )

const renderPage = (edit: boolean) =>
  render(
    <ApplicationDetailPage
      detail={detailFixture(edit)}
      tab="flow"
      onTabChange={() => undefined}
      onBack={() => undefined}
    />,
    {
      wrapper,
    }
  )

test('no Edit menu and a read-only notice when capabilities.edit is false', () => {
  const view = renderPage(false)

  assert.equal(view.queryByRole('button', { name: 'edit' }), null)
  assert.ok(view.getByText('config_notice'))
})

test('the Edit menu is present when capabilities.edit is true', () => {
  const view = renderPage(true)

  assert.ok(view.getByRole('button', { name: 'edit' }))
  assert.equal(view.queryByText('config_notice'), null)
})
