import {
  ActionIcon,
  Avatar,
  Box,
  Button,
  Divider,
  Group,
  NavLink,
  ScrollArea,
  Stack,
  Text,
  ThemeIcon,
  Tooltip,
} from '@mantine/core'
import { modals } from '@mantine/modals'
import { IconAlertTriangle, IconEdit, IconPlugConnected, IconPlus, IconTrash } from '@tabler/icons-react'
import { useCallback, useEffect, useState } from 'react'
import { Link, useLocation, useSearchParams } from 'react-router-dom'

import { useIsMobile } from '../../hooks/useIsMobile'
import { logger } from '../../lib/logger'
import { useProviderDetails } from '../../queries/useProviderDetailsQuery'
import { useAddProvider, useProviders, useRemoveProvider, useUpdateProvider } from '../../queries/useProvidersQueries'
import { useProviderStore } from '../../stores/providerStore'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { formatCount } from '../../utils/formatCount'
import { AppLogo } from '../atoms/AppLogo'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { AddProviderModal } from '../provider/AddProviderModal'

interface NavbarProps {
  onToggleSidebar?: () => void
  sidebarOpened?: boolean
}

export const Navbar = ({ onToggleSidebar, sidebarOpened = true }: NavbarProps) => {
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
    async (id: number, config: import('../../types').ProviderConfig) => {
      await updateProviderMutation({ id, config })
    },
    [updateProviderMutation]
  )

  const handleAddProvider = useCallback(
    async (config: import('../../types').ProviderConfig) => {
      await addProviderMutation(config)
    },
    [addProviderMutation]
  )

  const handleCloseAddModal = useCallback(() => {
    closeAddProviderModal()
    refetchProviders()
  }, [closeAddProviderModal, refetchProviders])

  const totalPipelines = providers.reduce((sum, p) => sum + p.pipeline_count, 0)

  return (
    <Stack h="100%" gap={0}>
      <ScrollArea flex={1} p="md">
        <Stack gap="xs">
          <NavLink
            leftSection={<AppLogo size={20} />}
            label="All providers"
            description={loading || providersError ? undefined : `${formatCount(totalPipelines, 'pipeline')} total`}
            active={pathname === '/pipelines' && providerParam === null}
            component={Link}
            to="/pipelines"
            onClick={closeMobileSidebar}
            variant="subtle"
          />

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
              <Group key={provider.id} gap={0} wrap="nowrap">
                <NavLink
                  flex={1}
                  miw={0}
                  leftSection={
                    provider.icon ? (
                      <Avatar src={provider.icon} size={20} radius="xs">
                        <IconPlugConnected size={14} />
                      </Avatar>
                    ) : (
                      <ThemeIcon size={20} radius="xs" variant="light" color="gray">
                        <IconPlugConnected size={14} />
                      </ThemeIcon>
                    )
                  }
                  label={
                    <Group gap={4} wrap="nowrap">
                      <Text size="sm" truncate>
                        {provider.name}
                      </Text>
                      {provider.last_fetch_status === 'error' && (
                        <Tooltip label={provider.last_fetch_error || 'Failed to fetch pipelines'} multiline w={300}>
                          <ThemeIcon variant="transparent" color="red" size={14} role="img" aria-label="Fetch failed">
                            <IconAlertTriangle size={14} />
                          </ThemeIcon>
                        </Tooltip>
                      )}
                    </Group>
                  }
                  description={formatCount(provider.pipeline_count, 'pipeline')}
                  active={providerParam === String(provider.id)}
                  component={Link}
                  to={`/pipelines?provider=${provider.id}`}
                  onClick={closeMobileSidebar}
                  variant="subtle"
                />
                <Group gap={4} wrap="nowrap" pr="xs">
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

      <Divider />
      <Box p="md" pb={isMobile ? 'calc(var(--mantine-spacing-md) + env(safe-area-inset-bottom))' : 'md'}>
        <Button
          leftSection={<IconPlus size={16} />}
          variant="default"
          size="sm"
          fullWidth
          onClick={openAddProviderModal}
        >
          Add provider
        </Button>
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
