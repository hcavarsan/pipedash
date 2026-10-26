import assert from 'node:assert/strict'
import { test } from 'node:test'

import { QueryClient } from '@tanstack/react-query'

import { queryKeys } from './queryKeys'

test('invalidating the run list of a pipeline marks every loaded page stale', async () => {
  const client = new QueryClient()
  const pipelineId = 'github__1__acme__app__42'
  const otherId = 'github__1__acme__web__7'
  client.setQueryData(queryKeys.runs.list(pipelineId, 1), { runs: [] })
  client.setQueryData(queryKeys.runs.list(pipelineId, 2), { runs: [] })
  client.setQueryData(queryKeys.runs.list(otherId, 1), { runs: [] })

  await client.invalidateQueries({ queryKey: queryKeys.runs.list(pipelineId) })

  assert.equal(client.getQueryState(queryKeys.runs.list(pipelineId, 1))?.isInvalidated, true)
  assert.equal(client.getQueryState(queryKeys.runs.list(pipelineId, 2))?.isInvalidated, true)
  assert.equal(client.getQueryState(queryKeys.runs.list(otherId, 1))?.isInvalidated, false)
})
