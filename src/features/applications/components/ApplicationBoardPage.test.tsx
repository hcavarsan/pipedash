import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import type { ApplicationBoard, BoardCopy } from '../../../types/applicationBoard'
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
  unassigned: 'unassigned',
  unassigned_empty: 'unassigned_empty',
  unassigned_drop: 'unassigned_drop',
  applications: 'applications',
  applications_empty: 'applications_empty',
  new_application_drop: 'new_application_drop',
  open_application: 'open_application',
  build: 'build',
  deploy: 'deploy',
  lane_empty: 'lane_empty',
  environments: 'environments',
  environments_empty: 'environments_empty',
  new_environment_drop: 'new_environment_drop',
  new_environment: 'new_environment',
  environment_name: 'environment_name',
  create_environment: 'create_environment',
  cancel: 'cancel',
})

const board = (): ApplicationBoard => ({
  unassigned: [{ pipeline_id: 'p-build', name: 'build-api', repository: 'acme/api', status: 'success' }],
  applications: [
    {
      id: 7,
      name: 'storefront',
      editable: true,
      notice: null,
      build: [],
      deploy: [{ pipeline_id: 'p-deploy', name: 'deploy-api', repository: 'acme/api', status: 'success' }],
      environments: [{ group_key: 't1', label: 'prod', targets: [{ target_key: 't1', name: 'prod' }] }],
    },
    {
      id: 8,
      name: 'from-config',
      editable: false,
      notice: 'config_notice',
      build: [],
      deploy: [],
      environments: [],
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
  assert.ok(storefront.getByText('lane_empty'))
  assert.equal(storefront.getByRole('link', { name: 'storefront' }).getAttribute('href'), '/applications/7')
})

test('An application defined in config shows its notice and offers no drop hints', async () => {
  mount()

  const config = within(view().getByRole('region', { name: 'from-config' }))
  assert.ok(config.getByText('config_notice'))
  assert.equal(config.queryByText('lane_empty'), null)
  assert.equal(config.queryByText('new_environment_drop'), null)
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
