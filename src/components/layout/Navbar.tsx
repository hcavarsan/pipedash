import {
  ActionIcon,
  Box,
  Button,
  Center,
  CloseButton,
  Divider,
  Group,
  Loader,
  NavLink,
  ScrollArea,
  Stack,
  Text,
  TextInput,
  ThemeIcon,
  Tooltip,
} from '@mantine/core'
import { useLocalStorage } from '@mantine/hooks'
import {
  IconAlertTriangle,
  IconChevronRight,
  IconEdit,
  IconFileSettings,
  IconFolder,
  IconFolderCode,
  IconFolders,
  IconPin,
  IconPinFilled,
  IconPinnedOff,
  IconPlus,
  IconSearch,
  IconStack2,
  IconTrash,
} from '@tabler/icons-react'
import { Fragment, type KeyboardEvent, type MouseEvent, type ReactNode, useEffect, useState } from 'react'
import { Link, useLocation, useSearchParams } from 'react-router-dom'
import { useProviderCancelItems } from '../../hooks/useCancelActiveRuns'
import { usePins, usePipelineViews, useSetPin } from '../../queries/usePipelineViewQueries'
import { useProviders } from '../../queries/useProvidersQueries'
import { usePipelineViewStore } from '../../stores/pipelineViewStore'
import { useProviderStore } from '../../stores/providerStore'
import type { PinKind, PipelineView, ProviderSummary, ViewKind } from '../../types'
import { providerBadge } from '../../utils/fetchInfo'
import { formatCount } from '../../utils/formatCount'
import { formatTime } from '../../utils/formatDateTime'
import {
  ancestorKeys,
  buildViewNodes,
  filterNavTree,
  isPinned,
  matchesQuery,
  type NavNode,
  pinTo,
} from '../../utils/navTree'
import { rootViewOf } from '../../utils/viewTree'
import { ProviderIcon } from '../atoms/ProviderIcon'
import { useContextMenu } from '../common/ContextMenu'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { useConfirmRemoveProvider } from '../provider/ProviderModals'
import { useViewMenuItems } from '../table/PipelineViewTabs'
import { fromOf } from './mobileChrome'

interface NavbarProps {
  compact?: boolean
}

const providerStatus = (provider: ProviderSummary) => {
  if (provider.paused_until) {
    return { text: `Paused until ${formatTime(provider.paused_until)}`, failed: false }
  }
  if (provider.last_fetch_status === 'error') {
    return { text: 'Fetch failed', failed: true }
  }

  return { text: formatCount(provider.pipeline_count, 'pipeline'), failed: false }
}

const ICON_TILE = { rail: 30, list: 28 } as const

const VIEW_ICONS: Partial<Record<ViewKind, typeof IconStack2>> = {
  pinned: IconPinFilled,
  custom: IconFolder,
  config: IconFileSettings,
}

const PIN_ICONS: Record<PinKind, typeof IconStack2> = {
  view: IconFolder,
  repository: IconFolderCode,
}

const PINNED_ID = 'pinned'

const PROVIDERS_KEY = 'providers'

const VIEWS_COLLAPSED_KEY = 'views:collapsed'

const PINNED_FALLBACK: NavNode = { key: `view:${PINNED_ID}`, name: 'Pinned', children: [] }

type MenuProps = { onContextMenu: (event: MouseEvent) => void; onKeyDown: (event: KeyboardEvent) => void }

const NavTile = ({ icon: Icon, size }: { icon: typeof IconStack2; size: number }) => (
  <ThemeIcon size={size} variant="transparent" color="gray" aria-hidden>
    <Icon size={size * 0.6} />
  </ThemeIcon>
)

export const ProviderAvatar = ({ provider, size }: { provider: ProviderSummary; size: number }): ReactNode => (
  <ProviderIcon name={provider.name} icon={provider.icon} size={size} badge={providerBadge(provider, Date.now())} />
)

export const ProviderStatusLine = ({ provider }: { provider: ProviderSummary }): ReactNode => {
  const { text, failed } = providerStatus(provider)

  return (
    <Text size="xs" c={failed ? 'red' : 'dimmed'} truncate>
      {text}
    </Text>
  )
}

interface TreeRowProps {
  depth: number
  label: string
  icon: ReactNode
  to?: string | undefined
  active: boolean
  description?: ReactNode
  expandable?: boolean | undefined
  open?: boolean | undefined
  onToggle?: (() => void) | undefined
  menu?: MenuProps | undefined
  actions?: ReactNode
}

const TreeRow = ({
  depth,
  label,
  icon,
  to,
  active,
  description,
  expandable,
  open,
  onToggle,
  menu,
  actions,
}: TreeRowProps) => {
  const row = {
    ...menu,
    flex: 1,
    miw: 0,
    className: 'item-primary',
    leftSection: icon,
    label: (
      <Text size="sm" truncate>
        {label}
      </Text>
    ),
    description,
    variant: 'light',
  } as const

  return (
    <Group
      gap={0}
      wrap="nowrap"
      className="nav-item interactive-surface stretched"
      ml={depth * 12}
      mod={{ current: active }}
    >
      {expandable ? (
        <ActionIcon
          size="xs"
          aria-label={`${open ? 'Collapse' : 'Expand'} ${label}`}
          aria-expanded={open}
          onClick={onToggle}
        >
          <IconChevronRight size={14} style={{ transform: open ? 'rotate(90deg)' : undefined }} />
        </ActionIcon>
      ) : (
        <Box w={18} flex="none" />
      )}
      {to ? (
        <NavLink {...row} active={active} aria-current={active ? 'page' : undefined} component={Link} to={to} />
      ) : (
        <NavLink
          {...row}
          component="button"
          type="button"
          aria-expanded={open}
          {...(onToggle ? { onClick: onToggle } : {})}
        />
      )}
      {actions}
    </Group>
  )
}

export const Navbar = ({ compact = false }: NavbarProps): ReactNode => {
  const { pathname, state } = useLocation()
  const [searchParams] = useSearchParams()
  const parent = pathname.startsWith('/pipelines/') ? (fromOf(state) ?? '/pipelines') : null
  const parentParams = parent ? new URLSearchParams(parent.split('?')[1] ?? '') : searchParams
  const inPipelines = pathname === '/pipelines' || parent !== null
  const viewParam = inPipelines ? parentParams.get('view') : null
  const providerParam = inPipelines && viewParam === null ? parentParams.get('provider') : null

  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const requestEditProvider = useProviderStore((s) => s.requestEditProvider)

  const {
    data: providers = [],
    isLoading: loading,
    error: providersError,
    refetch: refetchProviders,
    isRefetching: refetchingProviders,
  } = useProviders()

  const confirmRemove = useConfirmRemoveProvider()
  const { data: views } = usePipelineViews()
  const { data: pins = [] } = usePins()
  const { mutate: setPin } = useSetPin()
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const contextMenu = useContextMenu()
  const { providerItems, dialog } = useProviderCancelItems(providers)
  const providerMenu = (provider: ProviderSummary) => contextMenu(() => providerItems(provider))
  const menuItems = useViewMenuItems(views ?? [], 'vertical')
  const pinItem = (kind: PinKind, ref: string, label: string) => {
    const pinned = isPinned(pins, kind, ref)

    return {
      label: `${pinned ? 'Unpin' : 'Pin'} ${label}`,
      icon: pinned ? <IconPinnedOff size={16} /> : <IconPin size={16} />,
      onClick: () => setPin({ kind, ref, pinned: !pinned }),
    }
  }
  const viewNodes = views ? buildViewNodes(views.filter((v) => v.kind !== 'all')) : [PINNED_FALLBACK]
  const activeRootId = (views && viewParam && rootViewOf(views, viewParam)?.id) || viewParam
  const viewTo = (id: string) => `/pipelines?view=${encodeURIComponent(id)}`
  const viewMenu = (view: PipelineView | undefined) => (view ? contextMenu(() => menuItems(view)) : undefined)

  const [expanded, setExpanded] = useLocalStorage<string[]>({
    key: 'pipedash-sidebar-expanded',
    defaultValue: [PROVIDERS_KEY],
  })
  const [query, setQuery] = useState('')
  const searching = query.trim() !== ''
  const isOpen = (key: string) => searching || expanded.includes(key)
  const toggle = (key: string) =>
    setExpanded((current) => (current.includes(key) ? current.filter((k) => k !== key) : [...current, key]))
  const activeKeys = viewParam ? ancestorKeys(viewNodes, `view:${viewParam}`) : null
  useEffect(() => {
    if (activeKeys?.some((key) => !expanded.includes(key))) {
      setExpanded((current) => [...new Set([...current, ...activeKeys])])
    }
  })
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

  const renderViewNode = (node: NavNode, depth: number): React.ReactNode => {
    const viewId = node.view?.id ?? PINNED_ID
    const open = isOpen(node.key)

    return (
      <Fragment key={node.key}>
        <TreeRow
          depth={depth}
          expandable={node.children.length > 0}
          open={open}
          onToggle={() => toggle(node.key)}
          label={node.name}
          icon={<NavTile icon={VIEW_ICONS[node.view?.kind ?? 'pinned'] ?? IconFolder} size={ICON_TILE.list} />}
          description={node.view ? formatCount(node.view.count, 'pipeline') : undefined}
          active={viewParam === viewId}
          to={viewTo(viewId)}
          menu={viewMenu(node.view)}
        />
        {open && node.children.map((child) => renderViewNode(child, depth + 1))}
      </Fragment>
    )
  }

  const totalPipelines = providers.reduce((sum, p) => sum + p.pipeline_count, 0)
  const allActive = inPipelines && providerParam === null && viewParam === null

  const railItem = (label: string, to: string, active: boolean, icon: React.ReactNode, menu?: MenuProps) => (
    <Tooltip label={label} position="right" key={to}>
      <ActionIcon
        {...menu}
        component={Link}
        to={to}
        size={44}
        radius="sm"
        className="interactive-surface"
        color={active ? 'blue' : 'gray'}
        mod={{ current: active }}
        aria-label={label}
        aria-current={active ? 'page' : undefined}
      >
        {icon}
      </ActionIcon>
    </Tooltip>
  )

  const content = compact ? (
    <ScrollArea flex={1} py="sm" scrollbarSize={4}>
      <Stack gap="2xs" align="center">
        {railItem(
          `All providers · ${formatCount(totalPipelines, 'pipeline')}`,
          '/pipelines',
          allActive,
          <NavTile icon={IconStack2} size={ICON_TILE.rail} />
        )}
        {viewNodes.map((node) =>
          railItem(
            node.view ? `${node.name} · ${formatCount(node.view.count, 'pipeline')}` : node.name,
            viewTo(node.view?.id ?? PINNED_ID),
            activeRootId === (node.view?.id ?? PINNED_ID),
            <NavTile icon={VIEW_ICONS[node.view?.kind ?? 'pinned'] ?? IconFolder} size={ICON_TILE.rail} />,
            viewMenu(node.view)
          )
        )}
        <Divider w={32} my="3xs" />
        {loading ? (
          <Center h={44}>
            <Loader size="sm" />
          </Center>
        ) : providersError ? (
          <Tooltip label="Couldn't load providers. Retry" position="right">
            <ActionIcon
              size={44}
              radius="sm"
              color="red"
              aria-label="Couldn't load providers. Retry"
              loading={refetchingProviders}
              onClick={() => refetchProviders()}
            >
              <IconAlertTriangle size={20} />
            </ActionIcon>
          </Tooltip>
        ) : (
          providers.map((provider) =>
            railItem(
              `${provider.name} · ${providerStatus(provider).text}`,
              `/pipelines?provider=${provider.id}`,
              providerParam === String(provider.id),
              <ProviderAvatar provider={provider} size={ICON_TILE.rail} />,
              providerMenu(provider)
            )
          )
        )}
      </Stack>
    </ScrollArea>
  ) : (
    <>
      <Box px="sm" pt="sm">
        <TextInput
          size="xs"
          placeholder="Search"
          aria-label="Search sidebar"
          value={query}
          leftSection={<IconSearch size={14} />}
          rightSection={query ? <CloseButton size="xs" aria-label="Clear search" onClick={() => setQuery('')} /> : null}
          onChange={(event) => setQuery(event.currentTarget.value)}
          onKeyDown={(event) => event.key === 'Escape' && setQuery('')}
        />
      </Box>
      <ScrollArea flex={1} px="sm" py="sm">
        <Stack gap="4xs">
          {pinnedNodes.map((node) => renderViewNode(node, 0))}
          {visiblePins.map((pin) => {
            const to = pinTo(pin)
            const active =
              pin.kind === 'view'
                ? viewParam === pin.ref
                : viewParam === null && providerParam === null && parentParams.get('search') === pin.ref

            return (
              <TreeRow
                key={`pin:${pin.kind}:${pin.ref}`}
                depth={1}
                label={pin.name}
                icon={<NavTile icon={PIN_ICONS[pin.kind]} size={ICON_TILE.list} />}
                active={active}
                to={to}
                menu={contextMenu(() => [pinItem(pin.kind, pin.ref, pin.kind)])}
              />
            )
          })}
          {showProviders && (
            <>
              <TreeRow
                depth={0}
                expandable
                open={isOpen(PROVIDERS_KEY)}
                onToggle={() => toggle(PROVIDERS_KEY)}
                label="All providers"
                icon={<NavTile icon={IconStack2} size={ICON_TILE.list} />}
                description={loading || providersError ? undefined : `${formatCount(totalPipelines, 'pipeline')} total`}
                active={allActive}
                to="/pipelines"
              />
              {isOpen(PROVIDERS_KEY) &&
                (loading ? (
                  <LoadingState message="Loading providers" />
                ) : providersError ? (
                  <ErrorState
                    title="Couldn't load providers"
                    error={providersError}
                    hint="Check your connection and provider credentials."
                    onRetry={() => refetchProviders()}
                    retrying={refetchingProviders}
                  />
                ) : providers.length === 0 ? (
                  <EmptyState title="No providers yet" description="Add a provider to see its pipelines here." />
                ) : (
                  visibleProviders.map((provider) => (
                    <TreeRow
                      key={provider.id}
                      depth={1}
                      label={provider.name}
                      icon={<ProviderAvatar provider={provider} size={ICON_TILE.list} />}
                      description={<ProviderStatusLine provider={provider} />}
                      active={providerParam === String(provider.id)}
                      menu={providerMenu(provider)}
                      to={`/pipelines?provider=${provider.id}`}
                      actions={
                        <Group gap="4xs" wrap="nowrap" className="nav-item-actions">
                          <Tooltip label={`Edit ${provider.name}`}>
                            <ActionIcon
                              size="sm"
                              aria-label={`Edit ${provider.name}`}
                              onClick={() => requestEditProvider(provider.id)}
                            >
                              <IconEdit size={14} />
                            </ActionIcon>
                          </Tooltip>
                          <Tooltip label={`Remove ${provider.name}`}>
                            <ActionIcon
                              size="sm"
                              aria-label={`Remove ${provider.name}`}
                              onClick={() => confirmRemove(provider.id, provider.name)}
                            >
                              <IconTrash size={14} />
                            </ActionIcon>
                          </Tooltip>
                        </Group>
                      }
                    />
                  ))
                ))}
            </>
          )}

          <Divider my="2xs" />

          {otherNodes.length > 0 && (
            <TreeRow
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
          {viewsOpen && otherNodes.map((node) => renderViewNode(node, 1))}
          {views && !searching && !views.some((view) => view.kind === 'custom') && (
            <Button
              leftSection={<IconPlus size={16} />}
              variant="default"
              size="xs"
              fullWidth
              onClick={() => openEditor(null)}
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

  return (
    <Stack h="100%" gap={0}>
      {content}
      {dialog}

      <Divider />
      <Box p={compact ? 'sm' : 'md'} pb={`calc(var(--mantine-spacing-${compact ? 'sm' : 'md'}) + var(--safe-bottom))`}>
        {compact ? (
          <Center>
            <Tooltip label="Add provider" position="right">
              <ActionIcon size={44} radius="sm" aria-label="Add provider" onClick={openAddProviderModal}>
                <IconPlus size={20} />
              </ActionIcon>
            </Tooltip>
          </Center>
        ) : (
          <Button
            leftSection={<IconPlus size={16} />}
            variant="default"
            size="sm"
            fullWidth
            onClick={openAddProviderModal}
          >
            Add provider
          </Button>
        )}
      </Box>
    </Stack>
  )
}
