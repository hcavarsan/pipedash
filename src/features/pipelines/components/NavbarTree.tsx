import {
  ActionIcon,
  Box,
  Button,
  CloseButton,
  Divider,
  Group,
  ScrollArea,
  Stack,
  Text,
  TextInput,
  Tooltip,
} from '@mantine/core'
import { useLocalStorage } from '@mantine/hooks'
import { IconEdit, IconFolder, IconFolders, IconPlus, IconSearch, IconStack2, IconTrash } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import type { PinnedItem, PipelineView, ProviderSummary } from '../../../types/domain'
import { formatCount } from '../../../utils/formatCount'
import { ProviderAvatar, ProviderStatusLine } from '../../providers/components/ProviderPresentation'
import { ancestorKeys, filterNavTree, matchesQuery, type NavNode, pinTo } from '../utils/navTree'
import { ICON_TILE, type MenuProps, NavbarTreeRow, NavTile, PIN_ICONS, VIEW_ICONS } from './NavbarTreeRow'
import { activeRootOf, flattenNavbarViewRows, PINNED_ID, viewNodesOf, viewTo } from './NavbarViewRows'

const PROVIDERS_KEY = 'providers'

const VIEWS_COLLAPSED_KEY = 'views:collapsed'

interface NavbarTreeProps {
  providers: ProviderSummary[]
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
  onRemoveProvider: (id: number, name: string) => void
  onCreateView: () => void
}

interface ProviderRowsProps {
  providers: ProviderSummary[]
  visibleProviders: ProviderSummary[]
  providerId: string | null
  loading: boolean
  error: unknown
  retrying: boolean
  onRetry: () => void
  providerMenu: (provider: ProviderSummary) => MenuProps
  onEditProvider: (id: number) => void
  onRemoveProvider: (id: number, name: string) => void
}

const ProviderRows = ({
  providers,
  visibleProviders,
  providerId,
  loading,
  error,
  retrying,
  onRetry,
  providerMenu,
  onEditProvider,
  onRemoveProvider,
}: ProviderRowsProps): ReactNode => {
  if (loading) {
    return <LoadingState message="Loading providers" />
  }

  if (error) {
    return (
      <ErrorState
        title="Couldn't load providers"
        error={error}
        hint="Check your connection and provider credentials."
        onRetry={onRetry}
        retrying={retrying}
      />
    )
  }

  if (providers.length === 0) {
    return <EmptyState title="No providers yet" description="Add a provider to see its pipelines here." />
  }

  return visibleProviders.map((provider) => (
    <NavbarTreeRow
      key={provider.id}
      depth={1}
      label={provider.name}
      icon={<ProviderAvatar provider={provider} size={ICON_TILE.list} />}
      description={<ProviderStatusLine provider={provider} />}
      active={providerId === String(provider.id)}
      menu={providerMenu(provider)}
      to={`/pipelines?provider=${provider.id}`}
      actions={
        <Group gap="4xs" wrap="nowrap" className="nav-item-actions">
          <Tooltip label={`Edit ${provider.name}`}>
            <ActionIcon size="sm" aria-label={`Edit ${provider.name}`} onClick={() => onEditProvider(provider.id)}>
              <IconEdit size={14} aria-hidden />
            </ActionIcon>
          </Tooltip>
          <Tooltip label={`Remove ${provider.name}`}>
            <ActionIcon
              size="sm"
              aria-label={`Remove ${provider.name}`}
              onClick={() => onRemoveProvider(provider.id, provider.name)}
            >
              <IconTrash size={14} aria-hidden />
            </ActionIcon>
          </Tooltip>
        </Group>
      }
    />
  ))
}

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
  pinMenu,
  onEditProvider,
  onRemoveProvider,
  onCreateView,
}: NavbarTreeProps): ReactNode => {
  const [expanded, setExpanded] = useLocalStorage<string[]>({
    key: 'pipedash-sidebar-expanded',
    defaultValue: [PROVIDERS_KEY],
  })
  const [query, setQuery] = useState('')
  const searching = query.trim() !== ''
  const viewNodes = viewNodesOf(views)
  const activeRootId = activeRootOf(views, viewId)
  const activeKeys = viewId ? ancestorKeys(viewNodes, `view:${viewId}`) : null
  const isOpen = (key: string) => searching || expanded.includes(key) || activeKeys?.includes(key) === true
  const toggle = (key: string) =>
    setExpanded((current) => (current.includes(key) ? current.filter((k) => k !== key) : [...current, key]))

  const pinnedNodes = filterNavTree(
    viewNodes.filter((node) => node.view?.kind === 'pinned' || !node.view),
    query
  )
  const otherNodes = filterNavTree(
    viewNodes.filter((node) => node.view && node.view.kind !== 'pinned'),
    query
  )
  const viewsOpen =
    searching ||
    !expanded.includes(VIEWS_COLLAPSED_KEY) ||
    otherNodes.some((node) => node.view && node.view.id === activeRootId)
  const visiblePins = pins.filter((pin) => matchesQuery(pin.name, query))
  const visibleProviders = providers.filter((provider) => matchesQuery(provider.name, query))
  const showProviders = !searching || matchesQuery('All providers', query) || visibleProviders.length > 0
  const totalPipelines = providers.reduce((sum, p) => sum + p.pipeline_count, 0)
  const allActive = inPipelines && providerId === null && viewId === null

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
        <Stack gap="4xs">
          {renderViewRows(pinnedNodes, 0)}
          {visiblePins.map((pin) => (
            <NavbarTreeRow
              key={`pin:${pin.kind}:${pin.ref}`}
              depth={1}
              label={pin.name}
              icon={<NavTile icon={PIN_ICONS[pin.kind]} size={ICON_TILE.list} />}
              active={
                pin.kind === 'view'
                  ? viewId === pin.ref
                  : viewId === null && providerId === null && repositorySearch === pin.ref
              }
              to={pinTo(pin)}
              menu={pinMenu(pin)}
            />
          ))}
          {showProviders && (
            <>
              <NavbarTreeRow
                depth={0}
                expandable
                open={isOpen(PROVIDERS_KEY)}
                onToggle={() => toggle(PROVIDERS_KEY)}
                label="All providers"
                icon={<NavTile icon={IconStack2} size={ICON_TILE.list} />}
                description={loading || error ? undefined : `${formatCount(totalPipelines, 'pipeline')} total`}
                active={allActive}
                to="/pipelines"
              />
              {isOpen(PROVIDERS_KEY) && (
                <ProviderRows
                  providers={providers}
                  visibleProviders={visibleProviders}
                  providerId={providerId}
                  loading={loading}
                  error={error}
                  retrying={retrying}
                  onRetry={onRetry}
                  providerMenu={providerMenu}
                  onEditProvider={onEditProvider}
                  onRemoveProvider={onRemoveProvider}
                />
              )}
            </>
          )}

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
              Create view
            </Button>
          )}
          {searching && !showProviders && pinnedNodes.length + otherNodes.length + visiblePins.length === 0 && (
            <Text size="sm" c="dimmed" ta="center" py="sm">
              No matches
            </Text>
          )}
        </Stack>
      </ScrollArea>
    </>
  )
}
