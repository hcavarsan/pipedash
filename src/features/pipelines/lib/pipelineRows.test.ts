import assert from 'node:assert/strict'
import { test } from 'node:test'

import { pipelineViewPageFixture, providerSummaryFixture, viewPipelineFixture } from '../../../testing/fixtures'
import type { ProviderSummary, ViewSection } from '../../../types/domain'

import { accessorSort, buildPipelineRows } from './pipelineRows'

const providers = [providerSummaryFixture({ id: 1, name: 'Zeta' }), providerSummaryFixture({ id: 2, name: 'Alpha' })]
const getProvider = (id: number): ProviderSummary | undefined => providers.find((provider) => provider.id === id)

function section(overrides: Partial<ViewSection>): ViewSection {
  return {
    id: 'repo:org/repo',
    kind: 'repository',
    title: 'org/repo',
    organization: 'org',
    provider_ids: [1, 2],
    status: null,
    status_pipeline_id: null,
    last_run: null,
    pending_fetch: false,
    pipelines: [viewPipelineFixture({ id: 'a', name: 'build' }), viewPipelineFixture({ id: 'b', name: 'deploy' })],
    ...overrides,
  }
}

test('a collapsed section yields only its heading row', () => {
  const page = pipelineViewPageFixture({ sections: [section({})] })
  const rows = buildPipelineRows({ page, isOpen: () => false, getProvider })

  assert.equal(rows.length, 1)
  const [heading] = rows

  assert.ok(heading?.type === 'section')
  assert.equal(heading.expanded, false)
  assert.equal(heading.pipelineCount, 2)
  assert.equal(heading.providerLabel, 'Alpha, Zeta')
})

test('an expanded section yields its heading followed by indented workflow rows', () => {
  const page = pipelineViewPageFixture({ sections: [section({})] })
  const rows = buildPipelineRows({ page, isOpen: () => true, getProvider })

  assert.deepEqual(
    rows.map((row) => [row.type, row.name, row.depth]),
    [
      ['section', 'org/repo', undefined],
      ['workflow', 'build', 1],
      ['workflow', 'deploy', 1],
    ]
  )
})

test('a flat section has no heading and always lists its workflows at depth zero', () => {
  const page = pipelineViewPageFixture({ sections: [section({ id: 'none', kind: 'none', title: 'All' })] })
  const rows = buildPipelineRows({ page, isOpen: () => false, getProvider })

  assert.deepEqual(
    rows.map((row) => [row.type, row.depth]),
    [
      ['workflow', 0],
      ['workflow', 0],
    ]
  )
})

test('workflow row ids are unique per section and report an unknown provider', () => {
  const pipeline = viewPipelineFixture({ id: 'shared', provider_id: 99 })
  const page = pipelineViewPageFixture({
    sections: [
      section({ id: 'view:a', kind: 'view', pipelines: [pipeline] }),
      section({ id: 'view:b', kind: 'view', pipelines: [pipeline] }),
    ],
  })
  const rows = buildPipelineRows({ page, isOpen: () => true, getProvider })
  const workflows = rows.filter((row) => row.type === 'workflow')

  assert.equal(new Set(rows.map((row) => row.id)).size, rows.length)
  assert.deepEqual(
    workflows.map((row) => row.providerLabel),
    ['Unknown', 'Unknown']
  )
})

test('status sections are titled with the status label', () => {
  const page = pipelineViewPageFixture({
    sections: [section({ id: 'status:failed', kind: 'status', title: 'failed', status: 'failed', pipelines: [] })],
  })
  const [heading] = buildPipelineRows({ page, isOpen: () => false, getProvider })

  assert.equal(heading?.name, 'Failed')
})

test('no page yields no rows', () => {
  assert.deepEqual(buildPipelineRows({ page: undefined, isOpen: () => true, getProvider }), [])
})

test('accessorSort maps column ids to sort keys and ignores the actions column', () => {
  assert.equal(accessorSort('organization'), 'organization')
  assert.equal(accessorSort('lastRun'), 'last_run')
  assert.equal(accessorSort('actions'), undefined)
})
