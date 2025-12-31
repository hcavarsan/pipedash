import { useCallback, useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'

import {
  ActionIcon,
  Avatar,
  Box,
  Button,
  Divider,
  Group,
  Image,
  Loader,
  NavLink,
  ScrollArea,
  Stack,
  Text,
  ThemeIcon,
  Tooltip,
} from '@mantine/core'
import { modals } from '@mantine/modals'
import {
  IconAlertTriangle,
  IconEdit,
  IconPlugConnected,
  IconPlus,
  IconTrash,
} from '@tabler/icons-react'

import { useIsMobile } from '../../hooks/useIsMobile'
import { useProviderDetails } from '../../queries/useProviderDetailsQuery'
import {
  useAddProvider,
  useProviders,
  useRemoveProvider,
  useUpdateProvider,
} from '../../queries/useProvidersQueries'
import { useFilterStore } from '../../stores/filterStore'
import { useProviderStore } from '../../stores/providerStore'
import { AddProviderModal } from '../provider/AddProviderModal'

interface NavbarProps {
  onToggleSidebar?: () => void
  sidebarOpened?: boolean
}

export const Navbar = ({
  onToggleSidebar,
  sidebarOpened = true,
}: NavbarProps) => {
  const navigate = useNavigate()
  const { isMobile } = useIsMobile()

  const selectedProviderId = useFilterStore((s) => s.selectedProviderId)
  const setSelectedProviderId = useFilterStore((s) => s.setSelectedProviderId)

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
  } = useProviders()

  const { mutateAsync: addProviderMutation } = useAddProvider()
  const { mutateAsync: updateProviderMutation } = useUpdateProvider()
  const { mutateAsync: removeProviderMutation } = useRemoveProvider()

  const error = providersError ? (providersError as Error).message : null

  const [selectedEditId, setSelectedEditId] = useState<number | null>(null)
  const { data: providerDetails } = useProviderDetails(selectedEditId)

  useEffect(() => {
    if (providerDetails && selectedEditId) {
      openEditProviderModal(providerDetails)
      setSelectedEditId(null)
    }
  }, [providerDetails, selectedEditId, openEditProviderModal])

  const handleProviderSelect = useCallback(
    (id: number | undefined) => {
      setSelectedProviderId(id)
      navigate(id ? `/pipelines?provider=${id}` : '/pipelines')

      if (isMobile && onToggleSidebar && sidebarOpened) {
        onToggleSidebar()
      }
    },
    [setSelectedProviderId, navigate, isMobile, onToggleSidebar, sidebarOpened]
  )

  const handleEdit = useCallback((e: React.MouseEvent, id: number) => {
    e.stopPropagation()
    setSelectedEditId(id)
  }, [])

  const handleRemove = useCallback(
    (e: React.MouseEvent, id: number, name: string) => {
      e.stopPropagation()
      modals.openConfirmModal({
        title: 'Remove Provider',
        children: (
          <Text size="md">
            Are you sure you want to remove provider &quot;{name}&quot;? This action cannot be
            undone.
          </Text>
        ),
        labels: { confirm: 'Remove', cancel: 'Cancel' },
        confirmProps: { color: 'red' },
        onConfirm: async () => {
          try {
            await removeProviderMutation(id)
          } catch (err) {
            console.error('Failed to remove provider:', err)
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

  return (
    <>
      <Stack h="100%" gap={0}>
        <Box p="md" pb={0}>
          <Text size="xs" fw={600} c="dimmed" tt="uppercase" mb="xs">
            Navigation
          </Text>
        </Box>

        <ScrollArea flex={1} px="md" pb="md">
          <Stack gap="xs">
            <NavLink
              leftSection={
                <Box
                  style={{
                    width: 20,
                    height: 20,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                  }}
                >
                  <Image
                    src="/app-icon.png"
                    alt="All Providers"
                    h={28}
                    w={28}
                    fit="contain"
                    style={{ borderRadius: 4 }}
                  />
                </Box>
              }
              label={
                <Group justify="space-between" wrap="nowrap" w="100%">
                  <Box style={{ flex: 1, overflow: 'hidden' }}>
                    <Text size="sm" truncate>
                      All Providers
                    </Text>
                    <Text size="xs" c="dimmed">
                      {providers.reduce((sum, p) => sum + p.pipeline_count, 0)} pipelines total
                    </Text>
                  </Box>
                </Group>
              }
              active={selectedProviderId === undefined}
              onClick={() => handleProviderSelect(undefined)}
              color="blue"
              variant="subtle"
            />

            {loading ? (
              <Box ta="center" py="md">
                <Loader size="sm" />
                <Text size="sm" c="dimmed" mt="xs">
                  Loading providers...
                </Text>
              </Box>
            ) : error ? (
              <Box ta="center" py="md">
                <Text size="sm" c="red" fw={500}>
                  Error loading providers
                </Text>
                <Text size="xs" c="dimmed" mt={4}>
                  {error}
                </Text>
              </Box>
            ) : providers.length === 0 ? (
              <Text size="sm" c="dimmed" ta="center" py="md">
                No providers configured
              </Text>
            ) : (
              providers.map((provider) => {
                const isActive = selectedProviderId === provider.id

                return (
                  <NavLink
                    key={provider.id}
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
                      <Group justify="space-between" wrap="nowrap" w="100%">
                        <Box style={{ flex: 1, overflow: 'hidden' }}>
                          <Group gap={4} wrap="nowrap">
                            <Text size="sm" truncate>
                              {provider.name}
                            </Text>
                            {provider.last_fetch_status === 'error' && (
                              <Tooltip
                                label={provider.last_fetch_error || 'Failed to fetch pipelines'}
                                multiline
                                w={300}
                                withArrow
                              >
                                <IconAlertTriangle
                                  size={14}
                                  color="var(--mantine-color-red-6)"
                                />
                              </Tooltip>
                            )}
                          </Group>
                          <Text size="xs" c="dimmed">
                            {`${provider.pipeline_count} pipelines`}
                          </Text>
                        </Box>
                        <Group gap={4}>
                          <ActionIcon
                            size="sm"
                            variant="subtle"
                            color="gray"
                            onClick={(e) => handleEdit(e, provider.id)}
                          >
                            <IconEdit size={14} />
                          </ActionIcon>
                          <ActionIcon
                            size="sm"
                            variant="subtle"
                            color="red"
                            onClick={(e) => handleRemove(e, provider.id, provider.name)}
                          >
                            <IconTrash size={14} />
                          </ActionIcon>
                        </Group>
                      </Group>
                    }
                    active={isActive}
                    onClick={() => handleProviderSelect(provider.id)}
                    color="blue"
                    variant="subtle"
                  />
                )
              })
            )}
          </Stack>
        </ScrollArea>

        {!isMobile && (
          <>
            <Divider />
            <Box p="md">
              <Button
                leftSection={<IconPlus size={14} />}
                variant="light"
                color="blue"
                size="sm"
                fullWidth
                onClick={openAddProviderModal}
              >
                Add Provider
              </Button>
            </Box>
          </>
        )}

        <AddProviderModal
          opened={addProviderModal.open}
          onClose={handleCloseAddModal}
          onAdd={handleAddProvider}
        />

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
    </>
  )
}
