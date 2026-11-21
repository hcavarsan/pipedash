import assert from 'node:assert/strict'
import { after, afterEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { RenderResult } from '@testing-library/react'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { service } = await import('../../../services/service')
const { NavbarRail } = await import('./NavbarRail')

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const noop = () => undefined
const menu = () => ({ onContextMenu: noop, onKeyDown: noop })

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})
after(() => GlobalRegistrator.unregister())

function rail(loading: boolean, error: unknown) {
  return (
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <NavbarRail
            providers={[]}
            viewNodes={[]}
            activeRootId={null}
            providerId={null}
            allActive={false}
            loading={loading}
            error={error}
            retrying={false}
            onRetry={noop}
            providerMenu={menu}
            viewMenu={() => undefined}
            readFailures={[]}
          />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
}

function stubTotal(context: TestContext) {
  context.mock.method(service, 'getPipelineNavigation', async () => ({
    views: [],
    pins: [],
    providers: [],
    total_pipelines: 12,
  }))
}

const allProvidersLabel = async (rendered: RenderResult) =>
  (await rendered.findByRole('link', { name: /All providers/ })).getAttribute('aria-label')

test('the collapsed all providers item shows no count while providers load or fail', async (context) => {
  stubTotal(context)

  const loading = render(rail(true, null))
  assert.equal(await allProvidersLabel(loading), 'All providers')
  loading.unmount()

  const failed = render(rail(false, new Error('offline')))
  assert.equal(await allProvidersLabel(failed), 'All providers')
})

test('the collapsed all providers item shows the backend total once providers load', async (context) => {
  stubTotal(context)

  const rendered = render(rail(false, null))
  await rendered.findByRole('link', { name: 'All providers · 12 pipelines' })
})
