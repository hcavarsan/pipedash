import { ActionIcon, Center, Divider, Loader, ScrollArea, Stack, Tooltip } from '@mantine/core'
import { IconAlertTriangle, IconStack2 } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'

import type { PipelineView, ProviderSummary } from '../../types'
import { formatCount } from '../../utils/formatCount'
import type { NavNode } from '../../utils/navTree'

import { ICON_TILE, type MenuProps, NavTile, VIEW_ICONS } from './NavbarTreeRow'
import { PINNED_ID, viewTo } from './NavbarViewRows'
import { ProviderAvatar, providerStatus } from './ProviderPresentation'

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
}

interface RailItemProps {
  label: string
  to: string
  active: boolean
  icon: ReactNode
  menu?: MenuProps | undefined
}

const RailItem = ({ label, to, active, icon, menu }: RailItemProps): ReactNode => (
  <Tooltip label={label} position="right">
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
        <Divider w={32} my="3xs" />
        {loading ? (
          <Center h={44}>
            <Loader size="sm" />
          </Center>
        ) : error ? (
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
        ) : (
          providers.map((provider) => (
            <RailItem
              key={provider.id}
              label={`${provider.name} · ${providerStatus(provider).text}`}
              to={`/pipelines?provider=${provider.id}`}
              active={providerId === String(provider.id)}
              icon={<ProviderAvatar provider={provider} size={ICON_TILE.rail} />}
              menu={providerMenu(provider)}
            />
          ))
        )}
      </Stack>
    </ScrollArea>
  )
}
