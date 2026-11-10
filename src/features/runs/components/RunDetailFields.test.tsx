import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { DataTableColumn } from 'mantine-datatable'

import { runPresentationFixture } from '../../../testing/presentation'
import type { PipelineRun } from '../../../types/pipelines'
import type { ColumnDefinition } from '../../../types/plugins'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { cleanup, render, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { queryKeys } = await import('../../../lib/queryKeys')
const { useTableColumns } = await import('../hooks/useTableColumns')
const { RunDetailFields, useDetailRows } = await import('./RunDetailFields')

const NO_EXTRA_COLUMNS: DataTableColumn<PipelineRun>[] = []
const HIDDEN_BY_DEFAULT = { columnVisibility: { reason: false, message: false } }

const column = (id: string, label: string, overrides: Partial<ColumnDefinition> = {}): ColumnDefinition => ({
  id,
  label,
  description: null,
  field_path: `metadata.${id}`,
  data_type: 'String',
  renderer: 'TruncatedText',
  visibility: 'WhenPresent',
  default_visible: false,
  width: null,
  sort: 'none',
  filterable: false,
  ...overrides,
})

const run: PipelineRun = {
  id: 'r1',
  pipeline_id: 'p1',
  run_number: 1,
  status: 'failed',
  started_at: '2026-10-25T12:00:00Z',
  concluded_at: null,
  duration_seconds: null,
  logs_url: '',
  commit_sha: null,
  commit_message: null,
  branch: null,
  actor: null,
  metadata: { reason: 'InstallFailed', message: 'timeout waiting for Deployment' },
  ...runPresentationFixture(),
}

function ListAndDetails() {
  const { columns } = useTableColumns(1, 'pipeline_runs', NO_EXTRA_COLUMNS, HIDDEN_BY_DEFAULT)
  const rows = useDetailRows(1, run)

  return (
    <>
      <ul aria-label="List columns">
        {columns.map((item) => (
          <li key={String(item.accessor)}>{String(item.title)}</li>
        ))}
      </ul>
      <RunDetailFields rows={rows} run={run} mobileShell={false} isMobile={false} />
    </>
  )
}

afterEach(() => cleanup())
after(() => GlobalRegistrator.unregister())

test('the reason and message a list hides are listed once the run is open', () => {
  const client = new QueryClient()
  client.setQueryData(queryKeys.tableSchema.detail(1), {
    tables: [
      {
        id: 'pipeline_runs',
        name: 'Runs',
        description: null,
        columns: [
          column('status', 'Status', {
            field_path: 'status',
            data_type: 'Status',
            renderer: 'StatusBadge',
            visibility: 'Always',
            default_visible: true,
          }),
          column('reason', 'Reason'),
          column('message', 'Message'),
        ],
      },
    ],
  })

  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <ListAndDetails />
      </QueryClientProvider>
    </MantineProvider>
  )
  const view = within(document.body)
  const listed = within(view.getByRole('list', { name: 'List columns' }))
    .getAllByRole('listitem')
    .map((item) => item.textContent)

  assert.deepEqual(listed, ['Status'])
  assert.ok(view.getByText('InstallFailed'))
  assert.ok(view.getByText('timeout waiting for Deployment'))
})
