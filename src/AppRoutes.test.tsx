import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })

const desktopAgent = navigator.userAgent
const phoneAgent = 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Mobile/15E148'

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter, useLocation } = await import('react-router-dom')
const { AppRoutes } = await import('./AppRoutes')

function Location() {
  const location = useLocation()

  return <output aria-label="Location">{location.pathname + location.search}</output>
}

afterEach(async () => {
  await act(async () => cleanup())
  Object.defineProperty(navigator, 'userAgent', { value: desktopAgent, configurable: true })
})

after(() => GlobalRegistrator.unregister())

for (const [shell, entry, expected] of [
  ['mobile', '/', '/browse'],
  ['desktop', '/', '/pipelines'],
  ['mobile', '/providers', '/browse'],
  ['desktop', '/browse', '/pipelines'],
] as const) {
  test(`${entry} opens ${expected} on the ${shell} shell`, async () => {
    Object.defineProperty(navigator, 'userAgent', {
      value: shell === 'mobile' ? phoneAgent : desktopAgent,
      configurable: true,
    })
    const ready = { loading: false, error: null, retrying: false }

    render(
      <MantineProvider env="test">
        <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
          <MemoryRouter initialEntries={[entry]}>
            <AppRoutes
              providers={[]}
              pipelines={[]}
              listPrerequisites={ready}
              detailPrerequisites={ready}
              onRetry={async () => {}}
              onBack={() => {}}
              onViewHistory={() => {}}
              onViewMetrics={() => {}}
              onViewRun={() => {}}
              onTrigger={() => {}}
            />
            <Location />
          </MemoryRouter>
        </QueryClientProvider>
      </MantineProvider>
    )

    await waitFor(() =>
      assert.equal(within(document.body).getByRole('status', { name: 'Location' }).textContent, expected)
    )
  })
}
