import {
  ActionIcon,
  Box,
  Button,
  Center,
  Divider,
  Group,
  Loader,
  NavLink,
  ScrollArea,
  Stack,
  Text,
  ThemeIcon,
  Tooltip,
} from '@mantine/core'
import { modals } from '@mantine/modals'
import { IconAlertTriangle, IconEdit, IconPlus, IconStack2, IconTrash } from '@tabler/icons-react'
import { useCallback, useEffect, useState } from 'react'
import { Link, useLocation, useSearchParams } from 'react-router-dom'

import { useIsMobile } from '../../hooks/useIsMobile'
import { logger } from '../../lib/logger'
import { useProviderDetails } from '../../queries/useProviderDetailsQuery'
import { useAddProvider, useProviders, useRemoveProvider, useUpdateProvider } from '../../queries/useProvidersQueries'
import { useProviderStore } from '../../stores/providerStore'
import type { ProviderConfig, ProviderSummary } from '../../types'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { formatCount } from '../../utils/formatCount'
import { ProviderIcon } from '../atoms/ProviderIcon'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { AddProviderModal } from '../provider/AddProviderModal'

interface NavbarProps {
  compact?: boolean
  onToggleSidebar?: () => void
  sidebarOpened?: boolean
}

const providerStatusText = (provider: ProviderSummary) =>
  provider.last_fetch_status === 'error' ? 'Fetch failed' : formatCount(provider.pipeline_count, 'pipeline')

const ICON_TILE = { rail: 30, list: 28 } as const

const AllProvidersIcon = ({ size }: { size: number }) => (
  <ThemeIcon size={size} radius="sm" variant="light" color="gray" aria-hidden>
    <IconStack2 size={size * 0.6} />
  </ThemeIcon>
)

const ProviderAvatar = ({ provider, size }: { provider: ProviderSummary; size: number }) => (
  <ProviderIcon name={provider.name} icon={provider.icon} size={size} failed={provider.last_fetch_status === 'error'} />
)

export const Navbar = ({ compact = false, onToggleSidebar, sidebarOpened = true }: NavbarProps) => {
  const { isMobile } = useIsMobile()

  const { pathname } = useLocation()
  const [searchParams] = useSearchParams()
  const providerParam = pathname === '/pipelines' ? searchParams.get('provider') : null

  const addProviderModal = useProviderStore((s) => s.addProviderModal)
  const editProviderModal = useProviderStore((s) => s.editProviderModal)
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const closeAddProviderModal = useProviderStore((s) => s.closeAddProviderModal)
  const openEditProviderModal = useProviderStore((s) => s.openEditProviderModal)
  const closeEditProviderModal = useProviderStore((s) => s.closeEditProviderModal)

  const {
    data: providers = [],
    isLoading: loading,
    error: providersError,
    refetch: refetchProviders,
    isRefetching: refetchingProviders,
  } = useProviders()

  const { mutateAsync: addProviderMutation } = useAddProvider()
  const { mutateAsync: updateProviderMutation } = useUpdateProvider()
  const { mutateAsync: removeProviderMutation } = useRemoveProvider()

  const [selectedEditId, setSelectedEditId] = useState<number | null>(null)
  const { data: providerDetails } = useProviderDetails(selectedEditId)

  useEffect(() => {
    if (providerDetails && selectedEditId) {
      openEditProviderModal(providerDetails)
      setSelectedEditId(null)
    }
  }, [providerDetails, selectedEditId, openEditProviderModal])

  const closeMobileSidebar = useCallback(() => {
    if (isMobile && onToggleSidebar && sidebarOpened) {
      onToggleSidebar()
    }
  }, [isMobile, onToggleSidebar, sidebarOpened])

  const handleRemove = useCallback(
    (id: number, name: string) => {
      modals.openConfirmModal({
        title: 'Remove provider',
        children: (
          <Text size="sm">
            Remove &quot;{name}&quot; and its pipelines from Pipedash? This action cannot be undone.
          </Text>
        ),
        labels: { confirm: 'Remove provider', cancel: 'Cancel' },
        confirmProps: { color: 'red' },
        onConfirm: async () => {
          try {
            await removeProviderMutation(id)
          } catch (err) {
            logger.error('Navbar', 'Failed to remove provider', err)
            displayErrorNotification(err, "Couldn't remove provider")
          }
        },
      })
    },
    [removeProviderMutation]
  )

  const handleUpdateProvider = useCallback(
    async (id: number, config: ProviderConfig) => {
      await updateProviderMutation({ id, config })
    },
    [updateProviderMutation]
  )

  const handleAddProvider = useCallback(
    async (config: ProviderConfig) => {
      await addProviderMutation(config)
    },
    [addProviderMutation]
  )

  const handleCloseAddModal = useCallback(() => {
    closeAddProviderModal()
    refetchProviders()
  }, [closeAddProviderModal, refetchProviders])

  const totalPipelines = providers.reduce((sum, p) => sum + p.pipeline_count, 0)
  const allActive = pathname === '/pipelines' && providerParam === null

  const railItem = (label: string, to: string, active: boolean, icon: React.ReactNode) => (
    <Tooltip label={label} position="right" key={to}>
      <ActionIcon
        component={Link}
        to={to}
        size={44}
        radius="md"
        variant={active ? 'light' : 'subtle'}
        color={active ? 'blue' : 'gray'}
        aria-label={label}
        aria-current={active ? 'page' : undefined}
      >
        {icon}
      </ActionIcon>
    </Tooltip>
  )

  const content = compact ? (
    <ScrollArea flex={1} py="sm" scrollbarSize={4}>
      <Stack gap={6} align="center">
        {railItem(
          `All providers · ${formatCount(totalPipelines, 'pipeline')}`,
          '/pipelines',
          allActive,
          <AllProvidersIcon size={ICON_TILE.rail} />
        )}
        <Divider w={32} my={4} />
        {loading ? (
          <Center h={44}>
            <Loader size="sm" />
          </Center>
        ) : providersError ? (
          <Tooltip label="Couldn't load providers. Click to retry." position="right">
            <ActionIcon
              size={44}
              radius="md"
              variant="light"
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
              `${provider.name} · ${providerStatusText(provider)}`,
              `/pipelines?provider=${provider.id}`,
              providerParam === String(provider.id),
              <ProviderAvatar provider={provider} size={ICON_TILE.rail} />
            )
          )
        )}
      </Stack>
    </ScrollArea>
  ) : (
    <ScrollArea flex={1} px="sm" py="sm">
      <Stack gap={2}>
        <NavLink
          leftSection={<AllProvidersIcon size={ICON_TILE.list} />}
          label="All providers"
          description={loading || providersError ? undefined : `${formatCount(totalPipelines, 'pipeline')} total`}
          active={allActive}
          component={Link}
          to="/pipelines"
          onClick={closeMobileSidebar}
          variant="light"
        />

        <Divider my={6} />

        {loading ? (
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
          providers.map((provider) => (
            <Group key={provider.id} gap={0} wrap="nowrap" className="nav-item">
              <NavLink
                flex={1}
                miw={0}
                leftSection={<ProviderAvatar provider={provider} size={ICON_TILE.list} />}
                label={
                  <Text size="sm" truncate>
                    {provider.name}
                  </Text>
                }
                description={
                  <Text size="xs" c={provider.last_fetch_status === 'error' ? 'red' : 'dimmed'} truncate>
                    {providerStatusText(provider)}
                  </Text>
                }
                active={providerParam === String(provider.id)}
                component={Link}
                to={`/pipelines?provider=${provider.id}`}
                onClick={closeMobileSidebar}
                variant="light"
              />
              <Group gap={2} wrap="nowrap" className="nav-item-actions">
                <Tooltip label={`Edit ${provider.name}`}>
                  <ActionIcon
                    size="sm"
                    variant="subtle"
                    color="gray"
                    aria-label={`Edit ${provider.name}`}
                    onClick={() => setSelectedEditId(provider.id)}
                  >
                    <IconEdit size={14} />
                  </ActionIcon>
                </Tooltip>
                <Tooltip label={`Remove ${provider.name}`}>
                  <ActionIcon
                    size="sm"
                    variant="subtle"
                    color="gray"
                    aria-label={`Remove ${provider.name}`}
                    onClick={() => handleRemove(provider.id, provider.name)}
                  >
                    <IconTrash size={14} />
                  </ActionIcon>
                </Tooltip>
              </Group>
            </Group>
          ))
        )}
      </Stack>
    </ScrollArea>
  )

  return (
    <Stack h="100%" gap={0}>
      {content}

      <Divider />
      <Box
        p={compact ? 'sm' : 'md'}
        pb={isMobile ? 'calc(var(--mantine-spacing-md) + env(safe-area-inset-bottom))' : undefined}
      >
        {compact ? (
          <Center>
            <Tooltip label="Add provider" position="right">
              <ActionIcon
                size={44}
                radius="md"
                variant="default"
                aria-label="Add provider"
                onClick={openAddProviderModal}
              >
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

      <AddProviderModal opened={addProviderModal.open} onClose={handleCloseAddModal} onAdd={handleAddProvider} />

      {editProviderModal.provider && (
        <AddProviderModal
          opened={editProviderModal.open}
          onClose={closeEditProviderModal}
          onUpdate={handleUpdateProvider}
          editMode
          existingProvider={editProviderModal.provider}
        />
      )}
    </Stack>
  )
}
