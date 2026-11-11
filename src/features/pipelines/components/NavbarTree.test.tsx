import assert from 'node:assert/strict'
import { after, afterEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { pipelineViewFixture } from '../../../testing/fixtures'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { service } = await import('../../../services/service')
const { NavbarTree } = await import('./NavbarTree')

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const noop = () => undefined
const menu = () => ({ onContextMenu: noop, onKeyDown: noop })
const views = [
  pipelineViewFixture({ id: 'platform', kind: 'custom', name: 'Platform' }),
  pipelineViewFixture({ id: 'api', kind: 'custom', name: 'API', parent: 'platform' }),
  pipelineViewFixture({ id: 'web', kind: 'custom', name: 'Web', parent: 'platform' }),
]

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  window.localStorage.clear()
})
after(() => GlobalRegistrator.unregister())

function tree(viewId: string) {
  return (
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <NavbarTree
            providers={[]}
            applicationsActive={false}
            views={views}
            pins={[]}
            viewId={viewId}
            providerId={null}
            repositorySearch={null}
            inPipelines
            loading={false}
            error={null}
            retrying={false}
            onRetry={noop}
            providerMenu={menu}
            viewMenu={menu}
            pinMenu={menu}
            onEditProvider={noop}
            onCreateView={noop}
            readFailures={[]}
          />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
}

function stubNavigation(context: TestContext) {
  context.mock.method(service, 'getPipelineNavigation', async () => ({
    views,
    pins: [],
    providers: [],
    total_pipelines: 0,
  }))
}

test('an active group can be collapsed and going to a sibling view opens it again', async (context) => {
  stubNavigation(context)
  const user = userEvent.setup({ document })
  const rendered = render(tree('api'))

  await rendered.findByRole('link', { name: /API/ })

  await user.click(rendered.getByRole('button', { name: 'Collapse Platform' }))
  assert.equal(rendered.queryByRole('link', { name: /API/ }), null)

  await user.click(rendered.getByRole('button', { name: 'Collapse All views' }))
  assert.equal(rendered.queryByRole('link', { name: /Platform/ }), null)

  rendered.rerender(tree('web'))

  await rendered.findByRole('link', { name: /Web/ })
  assert.ok(rendered.getByRole('link', { name: /Platform/ }))
})
