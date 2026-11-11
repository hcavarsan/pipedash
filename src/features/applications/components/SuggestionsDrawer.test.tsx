import assert from 'node:assert/strict'
import { after, afterEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import type { Suggestion } from '../../../types/applications'
import { copyFixture } from './applicationFixtures'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { service } = await import('../../../services/service')
const { SuggestionsDrawer } = await import('./SuggestionsDrawer')

const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

const suggestion = (id: string, bulk_accept: boolean): Suggestion => ({
  id,
  application_id: null,
  kind: 'same_application',
  title: `title-${id}`,
  why: [],
  bulk_accept,
})

function mount(context: TestContext, suggestions: Suggestion[]) {
  const accepted: string[][] = []
  context.mock.method(service, 'listApplications', async () => ({
    applications: [],
    total: 0,
    suggestion_count: suggestions.length,
    config_error: null,
    copy: copyFixture(),
  }))
  context.mock.method(service, 'listApplicationSuggestions', async () => suggestions)
  context.mock.method(service, 'acceptApplicationSuggestions', async (ids: string[]) => {
    accepted.push(ids)
  })
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <SuggestionsDrawer opened onClose={() => undefined} />
      </QueryClientProvider>
    </MantineProvider>
  )

  return accepted
}

test('Accept all sends exactly the suggestions the backend marks bulk_accept', async (context) => {
  const accepted = mount(context, [suggestion('a', false), suggestion('b', true), suggestion('c', false)])

  const button = await view().findByRole('button', { name: 'accept_all' })
  await waitFor(() => assert.equal(button.hasAttribute('disabled'), false))
  await userEvent.click(button)

  await waitFor(() => assert.deepEqual(accepted, [['b']]))
})

test('Accept all is disabled when no suggestion is bulk_accept', async (context) => {
  const accepted = mount(context, [suggestion('a', false), suggestion('b', false)])

  await view().findByText('title-a')
  const button = await view().findByRole('button', { name: 'accept_all' })

  assert.equal(button.hasAttribute('disabled'), true)
  await userEvent.click(button)
  assert.deepEqual(accepted, [])
})
