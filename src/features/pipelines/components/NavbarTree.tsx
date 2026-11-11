import { Box, Button, CloseButton, Divider, ScrollArea, Stack, TextInput } from '@mantine/core'
import { IconFolder, IconFolders, IconPlus, IconSearch, IconStack2, IconTopologyStar3 } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { useDebounce } from '../../../hooks/useDebounce'
import type { ProviderSummary } from '../../../types/providers'
import type { PinnedItem, PipelineView } from '../../../types/views'
import { formatCount } from '../../../utils/formatCount'
import { PROVIDERS_KEY, useSidebarExpansion, VIEWS_COLLAPSED_KEY } from '../hooks/useSidebarExpansion'
import { usePipelineNavigation } from '../queries/usePipelineViewQueries'
import { ancestorKeys, type NavNode, pinTo } from '../utils/navTree'
import { ProviderRows } from './NavbarProviderRows'
import { NavbarReadFailures, type ReadFailure } from './NavbarReadFailures'
import { ICON_TILE, type MenuProps, NavbarTreeRow, NavTile, PIN_ICONS, VIEW_ICONS } from './NavbarTreeRow'
import { activeRootOf, flattenNavbarViewRows, PINNED_ID, viewNodesOf, viewTo } from './NavbarViewRows'

interface NavbarTreeProps {
  providers: ProviderSummary[]
  applicationsActive: boolean
  views: PipelineView[] | undefined
  pins: PinnedItem[]
  viewId: string | null
  providerId: string | null
  repositorySearch: string | null
  inPipelines: boolean
  loading: boolean
  error: unknown
  retrying: boolean
  onRetry: () => void
  providerMenu: (provider: ProviderSummary) => MenuProps
  viewMenu: (view: PipelineView | undefined) => MenuProps | undefined
  pinMenu: (pin: PinnedItem) => MenuProps
  onEditProvider: (id: number) => void
  onCreateView: () => void
  readFailures: ReadFailure[]
}

const PinRow = ({ pin, active, menu }: { pin: PinnedItem; active: boolean; menu: MenuProps }): ReactNode => (
  <NavbarTreeRow
    depth={1}
    label={pin.name}
    icon={<NavTile icon={PIN_ICONS[pin.kind]} size={ICON_TILE.list} />}
    active={active}
    to={pinTo(pin)}
    menu={menu}
  />
)

export const NavbarTree = ({
  providers,
  views,
  pins,
  viewId,
  providerId,
  repositorySearch,
  inPipelines,
  loading,
  error,
  retrying,
  onRetry,
  providerMenu,
  viewMenu,
  applicationsActive,
  pinMenu,
  onEditProvider,
  onCreateView,
  readFailures,
}: NavbarTreeProps): ReactNode => {
  const [query, setQuery] = useState('')
  const trimmed = query.trim()
  const searching = trimmed !== ''
  const debounced = useDebounce(trimmed)
  const search = usePipelineNavigation(debounced)
  const navigation = searching || (debounced === '' && !search.isPlaceholderData) ? search.data : undefined
  const searchFailed = searching && search.isError
  const searchBusy = debounced !== trimmed || (searching && (search.isFetching || search.isPlaceholderData))
  const navViews = navigation?.views ?? views
  const viewNodes = viewNodesOf(navViews)
  const baseNodes = viewNodesOf(views)
  const activeRootId = activeRootOf(views, viewId)
  const pinnedNodes = viewNodes.filter((node) => node.view?.kind === 'pinned' || !node.view)
  const otherNodes = viewNodes.filter((node) => node.view && node.view.kind !== 'pinned')
  const { isOpen, toggle, viewsOpen } = useSidebarExpansion({
    searching,
    viewId,
    revealKeys: (viewId ? ancestorKeys(baseNodes, `view:${viewId}`) : null) ?? [],
    revealViews: baseNodes.some((node) => !!node.view && node.view.kind !== 'pinned' && node.view.id === activeRootId),
  })
  const visiblePins = navigation?.pins ?? pins
  const visibleProviders = navigation?.providers ?? providers
  const showProviders = !searching || visibleProviders.length > 0
  const totalPipelines = navigation?.total_pipelines
  const allActive = inPipelines && providerId === null && viewId === null

  const isPinActive = (pin: PinnedItem) =>
    pin.kind === 'view' ? viewId === pin.ref : viewId === null && providerId === null && repositorySearch === pin.ref

  const renderViewRows = (nodes: NavNode[], depth: number) =>
    flattenNavbarViewRows(nodes, depth, isOpen).map(({ key, node, depth: rowDepth }) => (
      <NavbarTreeRow
        key={key}
        depth={rowDepth}
        expandable={node.children.length > 0}
        open={isOpen(node.key)}
        onToggle={() => toggle(node.key)}
        label={node.name}
        icon={<NavTile icon={VIEW_ICONS[node.view?.kind ?? 'pinned'] ?? IconFolder} size={ICON_TILE.list} />}
        description={node.view ? formatCount(node.view.count, 'pipeline') : undefined}
        active={viewId === (node.view?.id ?? PINNED_ID)}
        to={viewTo(node.view?.id ?? PINNED_ID)}
        menu={viewMenu(node.view)}
      />
    ))

  const providersSection = showProviders && (
    <>
      <NavbarTreeRow
        depth={0}
        expandable
        open={isOpen(PROVIDERS_KEY)}
        onToggle={() => toggle(PROVIDERS_KEY)}
        label="All providers"
        icon={<NavTile icon={IconStack2} size={ICON_TILE.list} />}
        description={
          loading || error || totalPipelines === undefined
            ? undefined
            : `${formatCount(totalPipelines, 'pipeline')} total`
        }
        active={allActive}
        to="/pipelines"
      />
      {isOpen(PROVIDERS_KEY) && (
        <ProviderRows
          {...{ providers, visibleProviders, providerId, loading, error, retrying, onRetry, providerMenu }}
          onEditProvider={onEditProvider}
        />
      )}
    </>
  )

  const tree = (
    <>
      {!searching && <NavbarReadFailures failures={readFailures} />}
      {renderViewRows(pinnedNodes, 0)}
      {visiblePins.map((pin) => (
        <PinRow key={`pin:${pin.kind}:${pin.ref}`} pin={pin} active={isPinActive(pin)} menu={pinMenu(pin)} />
      ))}
      <NavbarTreeRow
        depth={0}
        label="Applications"
        icon={<NavTile icon={IconTopologyStar3} size={ICON_TILE.list} />}
        active={applicationsActive}
        to="/applications"
      />
      {providersSection}

      <Divider my="2xs" />

      {otherNodes.length > 0 && (
        <NavbarTreeRow
          depth={0}
          expandable
          open={viewsOpen}
          onToggle={() => toggle(VIEWS_COLLAPSED_KEY)}
          label="All views"
          icon={<NavTile icon={IconFolders} size={ICON_TILE.list} />}
          description={formatCount(otherNodes.length, 'view')}
          active={false}
        />
      )}
      {viewsOpen && renderViewRows(otherNodes, 1)}
      {views && !searching && !views.some((view) => view.kind === 'custom') && (
        <Button
          leftSection={<IconPlus size={16} aria-hidden />}
          variant="default"
          size="xs"
          fullWidth
          onClick={onCreateView}
        >
          New view…
        </Button>
      )}
      {searching && !showProviders && pinnedNodes.length + otherNodes.length + visiblePins.length === 0 && (
        <EmptyState
          title="No matches"
          description="Nothing in the sidebar matches your search."
          action={
            <Button size="xs" variant="default" onClick={() => setQuery('')}>
              Clear search
            </Button>
          }
        />
      )}
    </>
  )

  const searchError = searchFailed ? (
    <ErrorState
      title="Couldn't search pipelines"
      error={search.error}
      onRetry={() => void search.refetch()}
      retrying={search.isFetching}
    />
  ) : null
  const results = (
    <>
      {searchError}
      {searching && !navigation ? null : tree}
    </>
  )

  return (
    <>
      <Box px="sm" pt="sm">
        <TextInput
          size="xs"
          placeholder="Search"
          aria-label="Search sidebar"
          value={query}
          leftSection={<IconSearch size={14} aria-hidden />}
          rightSection={query ? <CloseButton size="xs" aria-label="Clear search" onClick={() => setQuery('')} /> : null}
          onChange={(event) => setQuery(event.currentTarget.value)}
          onKeyDown={(event) => event.key === 'Escape' && setQuery('')}
        />
      </Box>
      <ScrollArea flex={1} px="sm" py="sm">
        <Stack gap="4xs" aria-busy={searchBusy}>
          {searchBusy && <LoadingState variant="compact" message="Searching…" />}
          {results}
        </Stack>
      </ScrollArea>
    </>
  )
}
