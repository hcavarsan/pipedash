import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })

const desktopAgent = navigator.userAgent
const phoneAgent = 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Mobile/15E148'

const { act, cleanup, render, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter, useLocation } = await import('react-router-dom')
const { AppRoutes } = await import('../../../AppRoutes')
const view = () => within(document.body)

function Location() {
  const location = useLocation()

  return (
    <output aria-label="Location">
      {location.pathname}
      {location.search}
    </output>
  )
}

function mount(shell: 'desktop' | 'mobile', retrying: boolean, onRetry: () => Promise<void>) {
  Object.defineProperty(navigator, 'userAgent', {
    value: shell === 'mobile' ? phoneAgent : desktopAgent,
    configurable: true,
  })
  const ready = { loading: false, error: null, retrying: false }

  render(
    <MantineProvider env="test">
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <MemoryRouter initialEntries={['/pipelines/github__1__owner%2Frepo%2Fbuild.yml']}>
          <AppRoutes
            providers={[]}
            pipelines={[]}
            listPrerequisites={ready}
            detailPrerequisites={{ loading: false, error: new Error('Backend unreachable'), retrying }}
            onRetry={onRetry}
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
}

afterEach(async () => {
  await act(async () => cleanup())
  Object.defineProperty(navigator, 'userAgent', { value: desktopAgent, configurable: true })
})

after(() => GlobalRegistrator.unregister())

for (const shell of ['desktop', 'mobile'] as const) {
  test(`a detail URL whose prerequisite read failed shows the error with Retry and stays put on the ${shell} shell`, async () => {
    let retries = 0
    mount(shell, false, async () => {
      retries += 1
    })

    await view().findByText("Couldn't load this pipeline")
    assert.ok(view().getByText('Backend unreachable'))
    assert.equal(view().queryByText("Couldn't find this pipeline"), null)
    assert.equal(
      view().getByRole('status', { name: 'Location' }).textContent,
      '/pipelines/github__1__owner%2Frepo%2Fbuild.yml'
    )

    await userEvent.setup({ document, delay: null }).click(view().getByRole('button', { name: 'Retry' }))
    assert.equal(retries, 1)
    assert.equal(
      view().getByRole('status', { name: 'Location' }).textContent,
      '/pipelines/github__1__owner%2Frepo%2Fbuild.yml'
    )
  })
}

test('Retry shows as busy while the prerequisite reads reload', async () => {
  mount('desktop', true, async () => {})

  await view().findByText("Couldn't load this pipeline")
  assert.equal(view().getByRole('button', { name: 'Retry' }).hasAttribute('data-loading'), true)
})
