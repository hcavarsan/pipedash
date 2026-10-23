import { Anchor, Button, Group, Stack, Text } from '@mantine/core'
import { IconExternalLink } from '@tabler/icons-react'

import type { Maintenance } from '../../types'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { formatDateTime } from '../../utils/formatDateTime'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { openExternal } from '../../utils/openExternal'
import type { ProviderIssue } from '../../utils/providerAttention'
import { ProviderRecoveryActions } from '../provider/ProviderRecoveryActions'

export const openLink = (url: string) => {
  openExternal(url).catch((error: unknown) => displayErrorNotification(error, "Couldn't open link"))
}

const maintenanceWindow = ({ starts_at, ends_at }: Maintenance): string | null => {
  if (starts_at && ends_at) {
    return `${formatDateTime(starts_at)} – ${formatDateTime(ends_at)}`
  }

  if (ends_at) {
    return `Until ${formatDateTime(ends_at)}`
  }

  return starts_at ? `Since ${formatDateTime(starts_at)}` : null
}

interface LinkedNameProps {
  name: string
  url: string | null
  ariaLabel: string
}

function LinkedName({ name, url, ariaLabel }: LinkedNameProps) {
  if (!url) {
    return (
      <Text size="sm" fw={500}>
        {name}
      </Text>
    )
  }

  return (
    <Anchor
      component="button"
      type="button"
      size="sm"
      fw={500}
      ta="left"
      underline="always"
      aria-label={ariaLabel}
      onClick={() => openLink(url)}
    >
      {name}
    </Anchor>
  )
}

interface ProviderStatusDetailsProps {
  issue: ProviderIssue
  now: number
  onNavigate: () => void
}

export function ProviderStatusDetails({ issue, now, onNavigate }: ProviderStatusDetailsProps) {
  const { provider, attention } = issue
  const { health } = provider
  const signal = health.signals.find((item) => `${provider.id}:${item.episode_id}` === attention.episodeKey)
  const statusPage = signal?.kind === 'status_page' ? health.status_page : null
  const incidents = signal?.incidents ?? []
  const maintenances = signal?.maintenances.filter((item) => item.in_progress) ?? []
  const affected = signal?.components.filter((item) => item.level !== 'operational').map((item) => item.name) ?? []
  const hasUpdate = incidents.some((item) => item.latest_update)
  const detail =
    attention.detail && !hasUpdate && !attention.hint?.startsWith(attention.detail) ? attention.detail : null
  const dataFrom = provider.last_fetch_status === 'error' ? provider.last_updated : null

  return (
    <Stack gap="xs">
      {signal && (
        <Text size="xs" c="dimmed">
          {signal.reported_by} · Updated {formatRelativeTime(signal.updated_at, now)}
        </Text>
      )}
      {incidents.map((incident) => (
        <Stack key={incident.id} gap={2}>
          <LinkedName
            name={incident.name}
            url={incident.url}
            ariaLabel={`View ${incident.name} on ${signal?.reported_by ?? provider.name}`}
          />
          {incident.latest_update && <Text size="sm">{incident.latest_update}</Text>}
        </Stack>
      ))}
      {detail && (
        <Text size="sm" style={{ overflowWrap: 'anywhere' }}>
          {detail}
        </Text>
      )}
      {affected.length > 0 && (
        <Text size="xs" c="dimmed">
          Affected: {affected.join(', ')}
        </Text>
      )}
      {maintenances.map((item) => {
        const window = maintenanceWindow(item)

        return (
          <Stack key={item.id} gap={2}>
            <LinkedName
              name={item.name}
              url={item.url}
              ariaLabel={`View ${item.name} on ${signal?.reported_by ?? provider.name}`}
            />
            {window && (
              <Text size="xs" c="dimmed">
                {window}
              </Text>
            )}
          </Stack>
        )
      })}
      {dataFrom && (
        <Text size="xs" c="dimmed">
          Showing data from {formatRelativeTime(dataFrom, now)}
        </Text>
      )}
      {attention.hint && <Text size="sm">{attention.hint}</Text>}
      {attention.staleNotes.map((note) => (
        <Text key={note} size="xs" c="dimmed">
          {note}
        </Text>
      ))}
      {attention.refreshingNormally && (
        <Text size="xs" c="dimmed">
          Pipedash is still refreshing normally
        </Text>
      )}
      <Group gap="xs" mt={4}>
        {statusPage && (
          <Button
            size="compact-xs"
            variant="default"
            leftSection={<IconExternalLink size={14} aria-hidden />}
            onClick={() => openLink(statusPage.url)}
          >
            Open status page
          </Button>
        )}
        <ProviderRecoveryActions providerId={provider.id} kind={attention.kind} onNavigate={onNavigate} />
      </Group>
    </Stack>
  )
}
