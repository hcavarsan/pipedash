import assert from 'node:assert/strict'
import { test } from 'node:test'

import { QueryClient } from '@tanstack/react-query'

import { queryKeys } from './queryKeys'
import { runHistoryQuery } from './runHistoryQuery'

test('invalidating the run list of a pipeline marks every loaded page stale', async () => {
  const client = new QueryClient()
  const pipelineId = 'github__1__acme__app__42'
  const otherId = 'github__1__acme__web__7'
  client.setQueryData(queryKeys.runs.list(pipelineId, runHistoryQuery(1, 20)), { runs: [] })
  client.setQueryData(queryKeys.runs.list(pipelineId, runHistoryQuery(2, 20)), { runs: [] })
  client.setQueryData(queryKeys.runs.list(otherId, runHistoryQuery(1, 20)), { runs: [] })

  await client.invalidateQueries({ queryKey: queryKeys.runs.list(pipelineId) })

  assert.equal(client.getQueryState(queryKeys.runs.list(pipelineId, runHistoryQuery(1, 20)))?.isInvalidated, true)
  assert.equal(client.getQueryState(queryKeys.runs.list(pipelineId, runHistoryQuery(2, 20)))?.isInvalidated, true)
  assert.equal(client.getQueryState(queryKeys.runs.list(otherId, runHistoryQuery(1, 20)))?.isInvalidated, false)
})
