import { useState } from 'react'

import {
  ActionIcon,
  Avatar,
  Box,
  Button,
  Divider,
  Group,
  Image,
  NavLink,
  ScrollArea,
  Stack,
  Text,
  ThemeIcon,
  Tooltip,
} from '@mantine/core'
import { modals } from '@mantine/modals'
import {
  IconChevronsLeft,
  IconEdit,
  IconPlugConnected,
  IconPlus,
  IconTrash,
} from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import { tauriService } from '../../services/tauri'
import type { ProviderConfig, ProviderSummary } from '../../types'
import { AddProviderModal } from '../provider/AddProviderModal'

interface NavbarProps {
  selectedProviderId?: number;
  onProviderSelect?: (id: number | undefined) => void;
  providers: ProviderSummary[];
  onAddProvider: (config: ProviderConfig) => Promise<void>;
  onUpdateProvider: (id: number, config: ProviderConfig) => Promise<void>;
  onRemoveProvider: (id: number, name: string) => Promise<void>;
  onToggleSidebar?: () => void;
  sidebarOpened?: boolean;
}

export const Navbar = ({
  selectedProviderId,
  onProviderSelect,
  providers,
  onAddProvider,
  onUpdateProvider,
  onRemoveProvider,
  onToggleSidebar,
  sidebarOpened = true,
}: NavbarProps) => {
  const isMobile = useIsMobile()
  const [addModalOpened, setAddModalOpened] = useState(false)
  const [editModalOpened, setEditModalOpened] = useState(false)
  const [editingProvider, setEditingProvider] = useState<(ProviderConfig & { id: number }) | null>(null)

  const handleProviderSelect = (id: number | undefined) => {
    if (onProviderSelect) {
      onProviderSelect(id)
    }
    if (isMobile && onToggleSidebar && sidebarOpened) {
      onToggleSidebar()
    }
  }

  const handleEdit = async (e: React.MouseEvent, id: number) => {
    e.stopPropagation()
    try {
      const providerConfig = await tauriService.getProvider(id)


      setEditingProvider(providerConfig)
      setEditModalOpened(true)
    } catch (error) {
      console.error('Failed to load provider:', error)
    }
  }

  const handleRemove = (e: React.MouseEvent, id: number, name: string) => {
    e.stopPropagation()
    modals.openConfirmModal({
      title: 'Remove Provider',
      children: (
        <Text size="md">
          Are you sure you want to remove provider &quot;{name}&quot;? This action cannot be undone.
        </Text>
      ),
      labels: { confirm: 'Remove', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: async () => {
        try {
          await onRemoveProvider(id, name)
        } catch (error) {
          console.error('Failed to remove provider:', error)
        }
      },
    })
  }

  return (
    <Stack h="100%" gap={0} style={{ position: 'relative' }}>
      {onToggleSidebar && sidebarOpened && !isMobile && (
        <Tooltip label="Hide sidebar" position="right" withArrow>
          <Box
            onClick={onToggleSidebar}
            style={{
              position: 'absolute',
              top: 4,
              right: -14,
              zIndex: 100,
              width: 28,
              height: 28,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              backgroundColor: 'var(--mantine-color-body)',
              border: '1px solid var(--mantine-color-default-border)',
              borderRadius: 6,
              cursor: 'pointer',
              transition: 'all 0.15s ease',
              boxShadow: '0 2px 4px rgba(0, 0, 0, 0.08)',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.backgroundColor = 'var(--mantine-color-gray-0)'
              e.currentTarget.style.transform = 'scale(1.05)'
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.backgroundColor = 'var(--mantine-color-body)'
              e.currentTarget.style.transform = 'scale(1)'
            }}
          >
            <IconChevronsLeft size={14} style={{ color: 'var(--mantine-color-dimmed)' }} />
          </Box>
        </Tooltip>
      )}

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
                  <Text size="sm" truncate>All Providers</Text>
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

          {providers.length === 0 ? (
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
                    <Avatar
                      src={provider.icon}
                      size={20}
                      radius="xs"
                    >
                      <IconPlugConnected size={14} />
                    </Avatar>
                  ) : (
                    <ThemeIcon
                      size={20}
                      radius="xs"
                      variant="light"
                      color="gray"
                    >
                      <IconPlugConnected size={14} />
                    </ThemeIcon>
                  )
                }
                label={
                  <Group justify="space-between" wrap="nowrap" w="100%">
                    <Box style={{ flex: 1, overflow: 'hidden' }}>
                      <Text size="sm" truncate>{provider.name}</Text>
                      <Text size="xs" c="dimmed">{provider.pipeline_count} pipelines</Text>
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
            <Text size="xs" fw={600} c="dimmed" tt="uppercase" mb="xs">
              Providers
            </Text>
            <Button
              leftSection={<IconPlus size={14} />}
              variant="light"
              color="blue"
              size="sm"
              fullWidth
              onClick={() => setAddModalOpened(true)}
            >
              Add Provider
            </Button>
          </Box>
        </>
      )}

      <AddProviderModal
        opened={addModalOpened}
        onClose={() => setAddModalOpened(false)}
        onAdd={onAddProvider}
      />

      {editingProvider && (
        <AddProviderModal
          opened={editModalOpened}
          onClose={() => {
            setEditModalOpened(false)
            setEditingProvider(null)
          }}
          onUpdate={onUpdateProvider}
          editMode={true}
          existingProvider={editingProvider}
        />
      )}
    </Stack>
  )
}
