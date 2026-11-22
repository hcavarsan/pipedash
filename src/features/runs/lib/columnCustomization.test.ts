import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { DataTableColumn } from 'mantine-datatable'

import type { PipelineRun } from '../../../types/pipelines'
import { buildColumnItems } from './columnCustomization'

const columns: DataTableColumn<PipelineRun>[] = ['status', 'branch', 'actor', 'duration', 'actions'].map(
  (accessor) => ({
    accessor,
    title: accessor,
  })
)

test('a column the saved order does not know yet is kept after the saved ones', () => {
  const items = buildColumnItems(columns, columns, ['actor', 'status', 'gone', 'actions'], { actor: false })

  assert.deepEqual(
    items.map((item) => item.id),
    ['actor', 'status', 'actions', 'branch', 'duration']
  )
  assert.equal(items.find((item) => item.id === 'actor')?.visible, false)
  assert.equal(items.find((item) => item.id === 'branch')?.visible, true)
})
