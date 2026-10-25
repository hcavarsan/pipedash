import { ActionIcon, Box, Button, Group, Menu, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconDots, IconEdit, IconPlus, IconStack2, IconTrash } from '@tabler/icons-react'
import { Link } from 'react-router-dom'

import { useProviders } from '../../queries/useProvidersQueries'
import { useProviderStore } from '../../stores/providerStore'
import { formatCount } from '../../utils/formatCount'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { ProviderAvatar, ProviderStatusLine } from '../layout/Navbar'

import { useConfirmRemoveProvider } from './ProviderModals'

const AVATAR = 32

export function ProvidersPage() {
  const { data: providers = [], isLoading, error, refetch, isRefetching } = useProviders()
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const requestEditProvider = useProviderStore((s) => s.requestEditProvider)
  const confirmRemove = useConfirmRemoveProvider()
  const totalPipelines = providers.reduce((sum, provider) => sum + provider.pipeline_count, 0)

  if (isLoading) {
    return <LoadingState message="Loading providers" />
  }

  if (error) {
    return (
      <ErrorState
        title="Couldn't load providers"
        error={error}
        hint="Check your connection and provider credentials."
        onRetry={() => refetch()}
        retrying={isRefetching}
      />
    )
  }

  if (providers.length === 0) {
    return (
      <EmptyState
        title="No providers yet"
        description="Add a provider to see its pipelines here."
        action={<Button onClick={openAddProviderModal}>Add provider</Button>}
      />
    )
  }

  return (
    <Stack gap="md" pb="md">
      <Box className="ios-group">
        <UnstyledButton className="ios-row" component={Link} to="/pipelines">
          <Group wrap="nowrap" gap="sm">
            <Box className="ios-icon-tile" aria-hidden>
              <IconStack2 size={18} />
            </Box>
            <Stack gap={0} flex={1} miw={0}>
              <Text size="sm" fw={500}>
                All providers
              </Text>
              <Text size="xs" c="dimmed">
                {formatCount(totalPipelines, 'pipeline')}
              </Text>
            </Stack>
          </Group>
        </UnstyledButton>
      </Box>

      <Box className="ios-group">
        {providers.map((provider) => (
          <Group key={provider.id} wrap="nowrap" gap={0} className="ios-row-shell">
            <UnstyledButton
              className="ios-row"
              style={{ flex: 1, minWidth: 0 }}
              component={Link}
              to={`/pipelines?provider=${provider.id}`}
            >
              <Group wrap="nowrap" gap="sm">
                <ProviderAvatar provider={provider} size={AVATAR} />
                <Stack gap={0} flex={1} miw={0}>
                  <Text size="sm" fw={500} truncate>
                    {provider.name}
                  </Text>
                  <ProviderStatusLine provider={provider} />
                </Stack>
              </Group>
            </UnstyledButton>

            <Menu position="bottom-end" withinPortal>
              <Menu.Target>
                <ActionIcon variant="subtle" color="gray" size={44} mr={4} aria-label={`Actions for ${provider.name}`}>
                  <IconDots size={18} />
                </ActionIcon>
              </Menu.Target>
              <Menu.Dropdown>
                <Menu.Item leftSection={<IconEdit size={16} />} onClick={() => requestEditProvider(provider.id)}>
                  Edit
                </Menu.Item>
                <Menu.Item
                  color="red"
                  leftSection={<IconTrash size={16} />}
                  onClick={() => confirmRemove(provider.id, provider.name)}
                >
                  Remove
                </Menu.Item>
              </Menu.Dropdown>
            </Menu>
          </Group>
        ))}
      </Box>

      <Box className="ios-group">
        <UnstyledButton className="ios-row ios-link-button" onClick={openAddProviderModal}>
          <Group gap="sm" wrap="nowrap">
            <IconPlus size={18} aria-hidden />
            <Text size="sm" fw={500} inherit>
              Add provider
            </Text>
          </Group>
        </UnstyledButton>
      </Box>
    </Stack>
  )
}
