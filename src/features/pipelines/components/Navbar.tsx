import { ActionIcon, Box, Button, Center, Divider, Stack, Tooltip } from '@mantine/core'
import { IconPlus } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { useLocation, useSearchParams } from 'react-router-dom'
import { useContextMenu } from '../../../components/common/ContextMenu'
import { fromOf } from '../../../components/layout/mobileChrome'
import { useProviders } from '../../../queries/useProvidersQueries'
import { useProviderStore } from '../../../stores/providerStore'
import type { ProviderSummary } from '../../../types/providers'
import type { PinKind, PipelineView } from '../../../types/views'
import { useNotifyItems } from '../../notifications/hooks/useNotifyItems'
import { useConfirmRemoveProvider } from '../../providers/components/ProviderModals'
import { useProviderCancelItems } from '../hooks/useCancelActiveRuns'
import { usePinning } from '../hooks/usePinning'
import { useViewMenuItems } from '../hooks/useViewMenuItems'
import { usePins, usePipelineViews } from '../queries/usePipelineViewQueries'
import { usePipelineViewStore } from '../stores/pipelineViewStore'
import { NavbarRail } from './NavbarRail'
import { NavbarTree } from './NavbarTree'
import { activeRootOf, viewNodesOf } from './NavbarViewRows'

interface NavbarProps {
  compact?: boolean
}

function AddProviderRailButton(p: { onClick: () => void }): ReactNode {
  return (
    <Center>
      <Tooltip label="Add provider" position="right">
        <ActionIcon size={44} radius="sm" aria-label="Add provider" onClick={p.onClick}>
          <IconPlus size={20} aria-hidden />
        </ActionIcon>
      </Tooltip>
    </Center>
  )
}

function AddProviderButton(p: { onClick: () => void }): ReactNode {
  return (
    <Button leftSection={<IconPlus size={16} aria-hidden />} variant="default" size="sm" fullWidth onClick={p.onClick}>
      Add provider
    </Button>
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
  const pinItem = usePinning()
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const contextMenu = useContextMenu()
  const { providerItems, dialog } = useProviderCancelItems(providers)
  const { providerItem } = useNotifyItems()
  const providerMenu = (provider: ProviderSummary) =>
    contextMenu(() => [...providerItems(provider), providerItem(provider)])
  const menuItems = useViewMenuItems(views ?? [], 'vertical')
  const viewMenu = (view: PipelineView | undefined) => (view ? contextMenu(() => menuItems(view)) : undefined)
  const pinMenu = (pin: { kind: PinKind; ref: string; name: string }) => contextMenu(() => [pinItem(pin.kind, pin.ref)])
  const onRetry = () => refetchProviders()

  const content = compact ? (
    <NavbarRail
      providers={providers}
      viewNodes={viewNodesOf(views)}
      activeRootId={activeRootOf(views, viewParam)}
      providerId={providerParam}
      allActive={inPipelines && providerParam === null && viewParam === null}
      loading={loading}
      error={providersError}
      retrying={refetchingProviders}
      onRetry={onRetry}
      providerMenu={providerMenu}
      viewMenu={viewMenu}
    />
  ) : (
    <NavbarTree
      providers={providers}
      views={views}
      pins={pins}
      viewId={viewParam}
      providerId={providerParam}
      repositorySearch={parentParams.get('search')}
      inPipelines={inPipelines}
      loading={loading}
      error={providersError}
      retrying={refetchingProviders}
      onRetry={onRetry}
      providerMenu={providerMenu}
      viewMenu={viewMenu}
      pinMenu={pinMenu}
      onEditProvider={requestEditProvider}
      onRemoveProvider={confirmRemove}
      onCreateView={() => openEditor(null)}
    />
  )

  return (
    <Stack h="100%" gap={0}>
      {content}
      {dialog}

      <Divider />
      <Box p={compact ? 'sm' : 'md'} pb={`calc(var(--mantine-spacing-${compact ? 'sm' : 'md'}) + var(--safe-bottom))`}>
        {compact ? (
          <AddProviderRailButton onClick={openAddProviderModal} />
        ) : (
          <AddProviderButton onClick={openAddProviderModal} />
        )}
      </Box>
    </Stack>
  )
}
