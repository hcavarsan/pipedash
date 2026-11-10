import { ActionIcon, Center, Divider, Loader, ScrollArea, Stack, Tooltip } from '@mantine/core'
import { IconAlertTriangle, IconStack2 } from '@tabler/icons-react'
import type { MouseEvent, ReactNode } from 'react'
import { Link } from 'react-router-dom'
import type { ProviderSummary } from '../../../types/providers'
import type { PipelineView } from '../../../types/views'
import { formatCount } from '../../../utils/formatCount'
import { ProviderAvatar, providerStatus } from '../../providers/components/ProviderPresentation'
import { useProviderNavigation } from '../../providers/queries/useProviderNavigation'
import type { NavNode } from '../utils/navTree'
import { RailReadFailures, type ReadFailure } from './NavbarReadFailures'
import { ICON_TILE, type MenuProps, NavTile, VIEW_ICONS } from './NavbarTreeRow'
import { PINNED_ID, viewTo } from './NavbarViewRows'

interface NavbarRailProps {
  providers: ProviderSummary[]
  viewNodes: NavNode[]
  activeRootId: string | null
  providerId: string | null
  allActive: boolean
  loading: boolean
  error: unknown
  retrying: boolean
  onRetry: () => void
  providerMenu: (provider: ProviderSummary) => MenuProps
  viewMenu: (view: PipelineView | undefined) => MenuProps | undefined
  readFailures: ReadFailure[]
}

interface RailItemProps {
  label: string
  to: string
  active: boolean
  icon: ReactNode
  menu?: MenuProps | undefined
  onClick?: ((event: MouseEvent<HTMLAnchorElement>) => void) | undefined
}

const RailItem = ({ label, to, active, icon, menu, onClick }: RailItemProps): ReactNode => (
  <Tooltip label={label} position="right">
    <ActionIcon
      {...menu}
      component={Link}
      to={to}
      onClick={onClick}
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

type RailProvidersProps = Pick<
  NavbarRailProps,
  'providers' | 'providerId' | 'loading' | 'error' | 'retrying' | 'onRetry' | 'providerMenu'
>

function RailProviders({
  providers,
  providerId,
  loading,
  error,
  retrying,
  onRetry,
  providerMenu,
}: RailProvidersProps): ReactNode {
  const openProvider = useProviderNavigation()

  if (loading) {
    return (
      <Tooltip label="Loading providers" position="right">
        <Center h={44} role="status" aria-label="Loading providers">
          <Loader size="xs" aria-hidden />
        </Center>
      </Tooltip>
    )
  }

  const retry = error ? (
    <Tooltip label="Couldn't load providers. Retry" position="right">
      <ActionIcon
        size={44}
        radius="sm"
        color="red"
        aria-label="Couldn't load providers. Retry"
        loading={retrying}
        onClick={onRetry}
      >
        <IconAlertTriangle size={20} aria-hidden />
      </ActionIcon>
    </Tooltip>
  ) : null

  return (
    <>
      {retry}
      {providers.map((provider) => (
        <RailItem
          key={provider.id}
          label={`${provider.name} · ${providerStatus(provider).text}`}
          to={`/pipelines?provider=${provider.id}`}
          active={providerId === String(provider.id)}
          icon={<ProviderAvatar provider={provider} size={ICON_TILE.rail} />}
          menu={providerMenu(provider)}
          onClick={(event) => openProvider(provider, event)}
        />
      ))}
    </>
  )
}

export const NavbarRail = ({
  providers,
  viewNodes,
  activeRootId,
  providerId,
  allActive,
  loading,
  error,
  retrying,
  onRetry,
  providerMenu,
  viewMenu,
  readFailures,
}: NavbarRailProps): ReactNode => {
  const totalPipelines = providers.reduce((sum, p) => sum + p.pipeline_count, 0)

  return (
    <ScrollArea flex={1} py="sm" scrollbarSize={4}>
      <Stack gap="2xs" align="center">
        <RailItem
          label={`All providers · ${formatCount(totalPipelines, 'pipeline')}`}
          to="/pipelines"
          active={allActive}
          icon={<NavTile icon={IconStack2} size={ICON_TILE.rail} />}
        />
        {viewNodes.map((node) => (
          <RailItem
            key={node.key}
            label={node.view ? `${node.name} · ${formatCount(node.view.count, 'pipeline')}` : node.name}
            to={viewTo(node.view?.id ?? PINNED_ID)}
            active={activeRootId === (node.view?.id ?? PINNED_ID)}
            icon={<NavTile icon={VIEW_ICONS[node.view?.kind ?? 'pinned'] ?? IconStack2} size={ICON_TILE.rail} />}
            menu={viewMenu(node.view)}
          />
        ))}
        <RailReadFailures failures={readFailures} />
        <Divider w={32} my="3xs" />
        <RailProviders
          providers={providers}
          providerId={providerId}
          loading={loading}
          error={error}
          retrying={retrying}
          onRetry={onRetry}
          providerMenu={providerMenu}
        />
      </Stack>
    </ScrollArea>
  )
}
