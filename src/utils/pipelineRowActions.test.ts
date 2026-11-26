import assert from 'node:assert/strict'
import { test } from 'node:test'

import { viewPipelineFixture } from '../testing/fixtures'
import type { PipelineActions, SectionMenuTarget } from '../types/pipelineRows'
import { rowMenuItems } from './pipelineRowActions'

const ignore = () => undefined

const sectionActions = (refreshed: number[][]): PipelineActions => ({
  onTogglePin: ignore,
  onToggleHidden: ignore,
  onSetMarks: ignore,
  isBusy: () => false,
  onAddToView: ignore,
  onTrigger: ignore,
  onEnable: ignore,
  onViewHistory: ignore,
  onOpenView: ignore,
  onRefreshProviders: (providerIds) => refreshed.push(providerIds),
  isRefreshing: () => false,
  pinItem: () => ({ label: 'Pin', icon: null, onClick: ignore }),
  cancelItem: () => null,
  notifyItem: () => ({ label: 'Notify', icon: null, onClick: ignore }),
  terminology: () => undefined,
})

const section = (kind: SectionMenuTarget['kind'], providerIds: number[]): SectionMenuTarget => ({
  type: 'section',
  id: kind === 'view' ? 'view:release' : 'repo:org/app',
  name: 'Release',
  kind,
  pipelines: [viewPipelineFixture()],
  providerIds: new Set(providerIds),
})

for (const kind of ['repository', 'view'] as const) {
  test(`a ${kind} section menu refreshes the providers the row button refreshes`, () => {
    const refreshed: number[][] = []
    const refresh = rowMenuItems(section(kind, [3, 7]), sectionActions(refreshed)).find(
      (item) => item.label === 'Refresh'
    )

    assert.ok(refresh)
    refresh.onClick()
    assert.deepEqual(refreshed, [[3, 7]])
  })

  test(`a ${kind} section without providers has no refresh item`, () => {
    const labels = rowMenuItems(section(kind, []), sectionActions([])).map((item) => item.label)

    assert.equal(labels.includes('Refresh'), false)
  })
}
