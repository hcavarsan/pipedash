import '../../../testing/registerDom'

import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, renderHook } from '@testing-library/react'
import type { DataTableColumn } from 'mantine-datatable'
import type { ReactNode } from 'react'
import { createElement } from 'react'
import { queryKeys } from '../../../lib/queryKeys'
import type { PipelineRun } from '../../../types/pipelines'
import type { ColumnDefinition } from '../../../types/plugins'
import { useTableColumns } from './useTableColumns'

const NO_EXTRA_COLUMNS: DataTableColumn<PipelineRun>[] = []

const column = (id: string): ColumnDefinition => ({
  id,
  label: id,
  description: null,
  field_path: id,
  data_type: 'String',
  renderer: 'TruncatedText',
  visibility: 'Always',
  default_visible: true,
  width: null,
  sort: 'none',
  filterable: false,
})

afterEach(() => cleanup())
after(() => GlobalRegistrator.unregister())

test('a schema column the saved order does not know yet stays in the table after the saved ones', () => {
  const client = new QueryClient()
  client.setQueryData(queryKeys.tableSchema.detail(1), {
    tables: [
      {
        id: 'pipeline_runs',
        name: 'Runs',
        description: null,
        columns: ['status', 'branch', 'actor', 'duration'].map(column),
      },
    ],
  })
  const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

  const { result } = renderHook(
    () =>
      useTableColumns(1, 'pipeline_runs', NO_EXTRA_COLUMNS, {
        columnOrder: ['actor', 'status', 'gone'],
        columnVisibility: { duration: false },
      }),
    { wrapper }
  )

  assert.deepEqual(
    result.current.columns.map((item) => String(item.accessor)),
    ['actor', 'status', 'branch']
  )
})
