import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import type { ApplicationBoard, BoardCopy, BoardLane, BoardPipeline, BoardZone } from '../../../types/applicationBoard'
import { copyFixture } from './applicationFixtures'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { ApplicationBoardPage } = await import('./ApplicationBoardPage')

const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

const boardCopy = (): BoardCopy => ({
  title: 'title',
  help: 'help',
  search_placeholder: 'search_placeholder',
  back: 'back',
  applications: 'applications',
  applications_empty: 'applications_empty',
  open_application: 'open_application',
  environments: 'environments',
  environments_empty: 'environments_empty',
  new_environment: 'new_environment',
  environment_name: 'environment_name',
  create_environment: 'create_environment',
  cancel: 'cancel',
})

const zone = (id: string, drop_class: string, enabled: boolean, hint: string | null): BoardZone => ({
  id,
  target: { kind: 'lane', application_id: null, key: null },
  drop_class,
  enabled,
  label: id,
  hint,
})

const pipeline = (key: string, name: string, zone_id: string, draggable: boolean): BoardPipeline => ({
  card: { item: { kind: 'pipeline', key, application_id: null }, zone_id, drop_class: 'pipeline', draggable },
  name,
  repository: 'acme/api',
  status: 'success',
})

const lane = (id: string, enabled: boolean, hint: string | null, pipelines: BoardPipeline[]): BoardLane => ({
  zone: zone(id, 'pipeline', enabled, hint),
  title: id,
  pipelines,
})

const board = (): ApplicationBoard => ({
  unassigned: lane('unassigned', true, 'unassigned_hint', [pipeline('p-build', 'build-api', 'unassigned', true)]),
  new_application: zone('new_application', 'pipeline', true, 'new_application_hint'),
  applications: [
    {
      id: 7,
      name: 'storefront',
      notice: null,
      build: lane('storefront build', true, 'lane_hint', []),
      deploy: lane('storefront deploy', true, null, [pipeline('p-deploy', 'deploy-api', 'storefront deploy', true)]),
      environments: [
        {
          zone: zone('storefront prod', 'target:7', true, null),
          label: 'prod',
          targets: [
            {
              card: {
                item: { kind: 'target', key: 't1', application_id: 7 },
                zone_id: 'storefront prod',
                drop_class: 'target:7',
                draggable: true,
              },
              name: 'prod-target',
            },
          ],
        },
      ],
      new_environment: zone('storefront new', 'target:7', true, 'new_environment_hint'),
    },
    {
      id: 8,
      name: 'from-config',
      notice: 'config_notice',
      build: lane('from-config build', false, null, []),
      deploy: lane('from-config deploy', false, null, []),
      environments: [],
      new_environment: null,
    },
  ],
  copy: boardCopy(),
})

function mount(): void {
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ApplicationBoardPage
            board={board()}
            applicationCopy={copyFixture()}
            search=""
            onSearchChange={() => undefined}
            onBack={() => undefined}
          />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
}

test('The board lists unassigned pipelines, each application lane and its environments', async () => {
  mount()

  const unassigned = within(view().getByRole('region', { name: 'unassigned' }))
  assert.ok(unassigned.getByText('build-api'))
  const storefront = within(view().getByRole('region', { name: 'storefront' }))
  assert.ok(storefront.getByText('deploy-api'))
  assert.ok(storefront.getByRole('group', { name: 'storefront prod' }))
  assert.ok(storefront.getByText('lane_hint'))
  assert.equal(storefront.getByRole('link', { name: 'storefront' }).getAttribute('href'), '/applications/7')
})

test('An application defined in config shows its notice and offers no drop hints', async () => {
  mount()

  const config = within(view().getByRole('region', { name: 'from-config' }))
  assert.ok(config.getByText('config_notice'))
  assert.equal(config.queryByText('lane_hint'), null)
  assert.equal(config.queryByText('new_environment_hint'), null)
})

test('Picking a pipeline up with the keyboard marks the lanes that accept it', async () => {
  mount()

  const card = view().getByText('build-api').closest('[role="button"]')
  assert.ok(card instanceof HTMLElement)
  await act(async () => {
    card.focus()
    fireEvent.keyDown(card, { code: 'Space' })
  })

  await waitFor(() => {
    const lane = view().getByRole('group', { name: 'storefront build' })
    assert.equal(lane.getAttribute('data-accepts'), 'true')
  })
  assert.equal(view().getByRole('group', { name: 'from-config build' }).getAttribute('data-accepts'), null)
  assert.equal(view().getByRole('group', { name: 'storefront prod' }).getAttribute('data-accepts'), null)
  assert.equal(view().getByRole('group', { name: 'unassigned' }).getAttribute('data-accepts'), null)
})
