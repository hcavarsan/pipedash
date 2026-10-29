import { STATUS_META } from '../../theme/status'
import type { PipelineViewPage, ProviderSummary, SectionMode, ViewSection, ViewSortKey } from '../../types'
import type { PipelineLabels } from '../../utils/pipelinePresentation'
import { typedKeys } from '../../utils/typedKeys'

import type { TableRow } from './PipelineRowParts'

export const collator = new Intl.Collator(undefined, { sensitivity: 'base', numeric: true })

export const SORT_ACCESSORS: Record<ViewSortKey, string | undefined> = {
  manual: undefined,
  name: 'name',
  last_run: 'lastRun',
  status: 'status',
  organization: 'organization',
  provider: 'provider',
}

export const accessorSort = (accessor: string): ViewSortKey | undefined =>
  typedKeys(SORT_ACCESSORS).find((key) => SORT_ACCESSORS[key] === accessor)

export const getSectionsLabel = (sections: SectionMode, labels: PipelineLabels, isFluxView: boolean): string =>
  ({
    repository: isFluxView ? 'Sources' : 'Repositories',
    organization: labels.groupTitle,
    provider: 'Providers',
    status: 'Statuses',
    none: labels.items.charAt(0).toUpperCase() + labels.items.slice(1),
  })[sections]

export const sectionTitle = (section: ViewSection): string =>
  section.kind === 'status' && section.status ? STATUS_META[section.status].label : section.title

interface BuildPipelineRowsInput {
  page: PipelineViewPage | undefined
  isOpen: (id: string, fallback: boolean) => boolean
  getProvider: (id: number) => ProviderSummary | undefined
}

export function buildPipelineRows({ page, isOpen, getProvider }: BuildPipelineRowsInput): TableRow[] {
  const rows: TableRow[] = []
  const providerLabel = (ids: number[]): string =>
    ids
      .map((id) => getProvider(id)?.name ?? 'Unknown')
      .sort(collator.compare)
      .join(', ')

  for (const section of page?.sections ?? []) {
    const flat = section.kind === 'none'
    const expanded = isOpen(section.id, false)

    if (!flat) {
      rows.push({
        id: section.id,
        type: 'section',
        kind: section.kind,
        name: sectionTitle(section),
        organization: section.organization ?? undefined,
        pipelineCount: section.pipelines.length,
        providerIds: new Set(section.provider_ids),
        providerLabel: providerLabel(section.provider_ids),
        hasPendingFetch: section.pending_fetch,
        lastRun: section.last_run,
        expanded,
        pipelines: section.pipelines,
      })

      if (!expanded) {
        continue
      }
    }

    for (const pipeline of section.pipelines) {
      rows.push({
        id: `workflow-${section.id}-${pipeline.id}`,
        type: 'workflow',
        name: pipeline.name,
        depth: flat ? 0 : 1,
        organization: pipeline.organization,
        providerIds: new Set([pipeline.provider_id]),
        providerLabel: getProvider(pipeline.provider_id)?.name ?? 'Unknown',
        status: pipeline.status,
        lastRun: pipeline.last_run,
        pipeline,
      })
    }
  }

  return rows
}
