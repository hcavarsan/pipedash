import { useCallback, useMemo, useState } from 'react'
import { useParams } from 'react-router-dom'
import type { PipelineSelection, TableRow } from '../../../components/common/PipelineRowParts'
import type { VirtualTableSort } from '../../../components/common/VirtualTable'
import { useDebounce } from '../../../hooks/useDebounce'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import { type PipelineFilters, usePipelineFilters, usePipelineViewQuery } from '../../../hooks/useUrlState'
import type {
  PinnedBlock,
  PipelineView,
  PipelineViewPage,
  PipelineViewQuery,
  ProviderSummary,
  ViewDisplay,
  ViewKind,
  ViewPipeline,
} from '../../../types/domain'
import { getPipelineLabels, type PipelineLabels } from '../../../utils/pipelinePresentation'
import {
  buildPhoneRows,
  buildPipelineRows,
  collator,
  getSectionsLabel,
  SORT_ACCESSORS,
  sectionTitle,
} from '../lib/pipelineRows'
import { usePipelineViewPage } from '../queries/usePipelineViewQueries'
import { type ExpansionMap, readExpansion, usePipelineExpansion, writeExpansion } from './usePipelineExpansion'
import { usePipelineSelection } from './usePipelineSelection'

interface PipelineViewModelInput {
  providers: ProviderSummary[]
  loading: boolean
  error: unknown
}

export interface PipelineViewPageModel {
  providers: ProviderSummary[]
  isMobile: boolean
  mobileShell: boolean
  group: string | null
  pushed: boolean
  mobileTitle: string | null
  loading: boolean
  error: unknown
  query: PipelineViewQuery
  page: PipelineViewPage | undefined
  view: PipelineView | undefined
  display: ViewDisplay | undefined
  kind: ViewKind
  title: string
  labels: PipelineLabels
  isFluxView: boolean
  filters: PipelineFilters
  setFilter: <K extends keyof PipelineFilters>(key: K, value: PipelineFilters[K]) => void
  selectStatus: (status: string, selected: boolean) => void
  clearFilters: () => void
  showHidden: boolean
  filtersActive: boolean
  isOpen: (id: string, fallback: boolean) => boolean
  toggleOpen: (id: string, fallback: boolean) => void
  toggleSection: (id: string) => void
  setAll: (ids: string[], open: boolean) => void
  getProvider: (providerId: number) => ProviderSummary | undefined
  providerOptions: { value: string; label: string }[]
  rows: TableRow[]
  collapsibleIds: string[]
  pinned: ViewPipeline[]
  pinnedBlocks: PinnedBlock[]
  hasPinnedArea: boolean
  sectionsLabel: string
  sectionCount: number
  selecting: boolean
  selectedPipelines: ViewPipeline[]
  selectedSet: Set<string>
  selection: PipelineSelection | undefined
  startSelection: () => void
  finishSelection: () => void
  selectRowKeys: (keys: ReadonlySet<string>) => void
  sortable: boolean
  sortStatus: VirtualTableSort
}

const pipelineIds = (records: TableRow[]): string[] =>
  records.flatMap((row) => (row.type === 'workflow' ? [row.pipeline.id] : []))

export function usePipelineViewModel({
  providers,
  loading: loadingProp,
  error: errorProp,
}: PipelineViewModelInput): PipelineViewPageModel {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const { group: groupParam } = useParams()
  const group = mobileShell ? (groupParam ?? null) : null
  const pushed = group !== null
  const { filters, setFilter, selectStatus, clearFilters } = usePipelineFilters()
  const urlQuery = usePipelineViewQuery()
  const debouncedSearch = useDebounce(filters.search)
  const query: PipelineViewQuery = { ...urlQuery, search: debouncedSearch || undefined }
  const {
    data: page,
    isLoading: pageLoading,
    error: pageError,
  } = usePipelineViewPage(query, providers.length > 0 && !loadingProp)
  const view = page?.view
  const display = view?.display
  const kind = view?.kind ?? (query.view === 'all' ? 'all' : query.view === 'pinned' ? 'pinned' : 'custom')
  const showHidden = filters.hidden === 'show'
  const activeProviders = filters.provider
    ? providers.filter((provider) => String(provider.id) === filters.provider)
    : providers
  const isFluxView =
    activeProviders.length > 0 && activeProviders.every((provider) => provider.provider_type === 'fluxcd')
  const labels = getPipelineLabels(isFluxView ? 'fluxcd' : undefined)
  const title = kind === 'all' ? (isFluxView ? 'FluxCD resources' : 'Pipelines') : (view?.name ?? 'Pipelines')
  const groupSection = page?.sections.find((section) => section.id === group)
  const providerName = filters.provider
    ? providers.find((provider) => String(provider.id) === filters.provider)?.name
    : undefined
  const listTitle = kind === 'all' ? (isFluxView ? 'FluxCD resources' : 'All pipelines') : (view?.name ?? null)
  const mobileTitle = !mobileShell ? null : groupSection ? sectionTitle(groupSection) : (providerName ?? listTitle)
  const filtersActive = page?.filters_active ?? false

  const [expanded, setExpandedMap] = useState<ExpansionMap>(readExpansion)
  const saveExpanded = (expansionView: string, changes: Record<string, boolean>) => {
    const next = { ...expanded, [expansionView]: { ...expanded[expansionView], ...changes } }

    setExpandedMap(next)
    writeExpansion(next)
  }
  const {
    isOpen,
    toggle: toggleOpen,
    setAll,
  } = usePipelineExpansion(query.view, expanded, saveExpanded, JSON.stringify(query), filtersActive)
  const toggleSection = (id: string) => toggleOpen(id, false)

  const getProvider = useCallback(
    (providerId: number): ProviderSummary | undefined => providers.find((p) => p.id === providerId),
    [providers]
  )
  const rows = useMemo(
    () =>
      mobileShell ? buildPhoneRows({ page, getProvider, group }) : buildPipelineRows({ page, isOpen, getProvider }),
    [page, isOpen, getProvider, mobileShell, group]
  )
  const providerOptions = useMemo(
    () =>
      (page?.facets.provider_ids ?? [])
        .map((id) => getProvider(id))
        .filter((provider): provider is ProviderSummary => provider !== undefined)
        .map((provider) => ({ value: String(provider.id), label: provider.name }))
        .sort((a, b) => collator.compare(a.label, b.label)),
    [page, getProvider]
  )
  const collapsibleIds = page?.sections.filter((section) => section.kind !== 'none').map((section) => section.id) ?? []

  const {
    selecting,
    selectedPipelines,
    toggle: toggleSelected,
    select,
    start: startSelection,
    finish: finishSelection,
  } = usePipelineSelection(query.view, page)
  const selectedSet = new Set(selectedPipelines.map((p) => p.id))
  const selection = selecting ? { selected: selectedSet, onToggle: toggleSelected } : undefined
  const selectRowKeys = (keys: ReadonlySet<string>) => {
    const dropped = new Set(
      pipelineIds(
        rows.filter((row) => row.type === 'workflow' && selectedSet.has(row.pipeline.id) && !keys.has(row.id))
      )
    )

    select(
      pipelineIds(rows),
      pipelineIds(rows.filter((row) => keys.has(row.id))).filter((id) => !dropped.has(id))
    )
  }

  const pinned = page?.pinned ?? []
  const pinnedBlocks =
    page?.pinned_blocks.filter((block) => block.kind === 'repository' && block.pipelines.length > 0) ?? []
  const sectionsLabel = getSectionsLabel(display?.sections ?? 'none', labels, isFluxView)
  const ownSections = page?.sections.filter((section) => section.kind !== 'view') ?? []
  const sectionCount =
    display?.sections === 'none'
      ? ownSections.reduce((sum, section) => sum + section.pipelines.length, 0)
      : ownSections.length

  return {
    providers,
    isMobile,
    mobileShell,
    group,
    pushed,
    mobileTitle,
    loading: loadingProp || pageLoading,
    error: errorProp ?? pageError,
    query,
    page,
    view,
    display,
    kind,
    title,
    labels,
    isFluxView,
    filters,
    setFilter,
    selectStatus,
    clearFilters,
    showHidden,
    filtersActive,
    isOpen,
    toggleOpen,
    toggleSection,
    setAll,
    getProvider,
    providerOptions,
    rows,
    collapsibleIds,
    pinned,
    pinnedBlocks,
    hasPinnedArea: pinned.length > 0 || pinnedBlocks.length > 0,
    sectionsLabel,
    sectionCount,
    selecting,
    selectedPipelines,
    selectedSet,
    selection,
    startSelection,
    finishSelection,
    selectRowKeys,
    sortable: view?.capabilities.edit_display ?? false,
    sortStatus: {
      columnId: (display && SORT_ACCESSORS[display.sort]) ?? 'manual',
      direction: display?.direction ?? 'asc',
    },
  }
}
