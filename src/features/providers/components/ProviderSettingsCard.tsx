import { ActionIcon, Button, Card, Divider, Group, SimpleGrid, Stack, Text, Tooltip } from '@mantine/core'
import { IconEdit, IconSettings, IconTrash } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { usePlugins } from '../../../queries/usePluginsQueries'
import { useProviderStore } from '../../../stores/providerStore'
import type { ProviderSummary } from '../../../types/domain'
import { formatCount } from '../../../utils/formatCount'
import { formatTime } from '../../../utils/formatDateTime'
import { getPluginDisplayName } from '../../../utils/pluginHelpers'
import { useConfirmRemoveProvider } from './ProviderModals'
import { ProviderRefreshIntervalForm } from './ProviderRefreshIntervalForm'
import { ProviderStatusInfo, ProviderStatusRows } from './ProviderStatus'

const PAUSED_NOTE = 'The API quota is almost used up. Pipedash resumes refreshing at the reset time.'

interface ProviderSettingsCardProps {
  provider: ProviderSummary
  statusPagesEnabled: boolean
  now: number
  editing: boolean
  onStartEditing: () => void
  onFinishEditing: () => void
}

export const ProviderSettingsCard = ({
  provider,
  statusPagesEnabled,
  now,
  editing,
  onStartEditing,
  onFinishEditing,
}: ProviderSettingsCardProps): ReactNode => {
  const mobileShell = useMobileShell()
  const { data: plugins } = usePlugins()
  const requestEditProvider = useProviderStore((s) => s.requestEditProvider)
  const confirmRemove = useConfirmRemoveProvider()
  const typeName = getPluginDisplayName(plugins, provider.provider_type)

  if (mobileShell) {
    return (
      <Stack gap="sm">
        <IosGroup>
          <IosRow
            title={provider.name}
            subtitle={[typeName, formatCount(provider.pipeline_count, 'pipeline')].join(' · ')}
          />
          {provider.last_fetch_status === 'error' && (
            <IosRow
              title={
                <Text span inherit c="red">
                  Fetch failed
                </Text>
              }
              subtitle={provider.last_fetch_error || 'Failed to fetch'}
            />
          )}
          {provider.paused_until && (
            <IosRow title={`Paused until ${formatTime(provider.paused_until)}`} subtitle={PAUSED_NOTE} />
          )}
          {editing ? (
            <ProviderRefreshIntervalForm provider={provider} onSaved={onFinishEditing} onCancel={onFinishEditing} />
          ) : (
            <IosRow
              title="Refresh interval"
              value={`${provider.refresh_interval}s`}
              chevron
              ariaLabel={`Edit refresh interval for ${provider.name}`}
              onClick={onStartEditing}
            />
          )}
          <ProviderStatusRows provider={provider} statusPagesEnabled={statusPagesEnabled} now={now} />
          <IosRow title="Edit provider" chevron onClick={() => requestEditProvider(provider.id)} />
        </IosGroup>
        <IosGroup>
          <IosRow
            title="Remove provider"
            destructive
            ariaLabel={`Remove ${provider.name}`}
            onClick={() => confirmRemove(provider.id, provider.name)}
          />
        </IosGroup>
      </Stack>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="md">
          <Stack gap="3xs">
            <Text size="xs" c="dimmed">
              Provider name
            </Text>
            <Group gap="xs">
              <Text size="sm">{provider.name}</Text>
              {provider.last_fetch_status === 'error' && (
                <Tooltip label={provider.last_fetch_error || 'Failed to fetch'} multiline w={300}>
                  <Text size="xs" c="red">
                    Error
                  </Text>
                </Tooltip>
              )}
              {provider.paused_until && (
                <Tooltip label={PAUSED_NOTE} multiline w={300}>
                  <Text size="xs" c="dimmed">
                    Paused until {formatTime(provider.paused_until)}
                  </Text>
                </Tooltip>
              )}
            </Group>
          </Stack>

          <Stack gap="3xs">
            <Text size="xs" c="dimmed">
              Type
            </Text>
            <Text size="sm">{typeName}</Text>
          </Stack>

          <Stack gap="3xs">
            <Text size="xs" c="dimmed">
              Pipelines
            </Text>
            <Text size="sm">{formatCount(provider.pipeline_count, 'pipeline')}</Text>
          </Stack>

          <Stack gap="3xs">
            <Text size="xs" c="dimmed">
              Refresh interval
            </Text>
            {editing ? (
              <ProviderRefreshIntervalForm provider={provider} onSaved={onFinishEditing} onCancel={onFinishEditing} />
            ) : (
              <Group gap="xs">
                <Text size="sm">{provider.refresh_interval}s</Text>
                <Tooltip label={`Edit refresh interval for ${provider.name}`}>
                  <ActionIcon
                    size="sm"
                    aria-label={`Edit refresh interval for ${provider.name}`}
                    onClick={onStartEditing}
                  >
                    <IconEdit size={14} aria-hidden />
                  </ActionIcon>
                </Tooltip>
              </Group>
            )}
          </Stack>
        </SimpleGrid>

        <ProviderStatusInfo provider={provider} statusPagesEnabled={statusPagesEnabled} now={now} />

        {!editing && (
          <>
            <Divider />
            <Group gap="xs" justify="flex-end">
              <Button
                size="compact-xs"
                variant="subtle"
                color="gray"
                onClick={() => requestEditProvider(provider.id)}
                leftSection={<IconSettings size={14} aria-hidden />}
              >
                Edit
              </Button>
              <Button
                size="compact-xs"
                variant="subtle"
                color="gray"
                onClick={() => confirmRemove(provider.id, provider.name)}
                leftSection={<IconTrash size={14} aria-hidden />}
              >
                Remove
              </Button>
            </Group>
          </>
        )}
      </Stack>
    </Card>
  )
}
