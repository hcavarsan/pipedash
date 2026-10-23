import { Alert, Anchor, Button, Group, Stack, Text, VisuallyHidden } from '@mantine/core'
import { IconAlertCircle, IconExternalLink, IconRefresh, IconSettings } from '@tabler/icons-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { Link } from 'react-router-dom'

import { useNow } from '../../hooks/useNow'
import { useProviders } from '../../queries/useProvidersQueries'
import { useRefreshProvider } from '../../queries/useRefreshQueries'
import { useHealthDismissalStore } from '../../stores/healthDismissalStore'
import { HEALTH_META, SIGNAL_ICONS } from '../../theme/status'
import type { FailureKind, HealthLevel, Maintenance, ProviderSummary } from '../../types'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { formatDateTime } from '../../utils/formatDateTime'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { openExternal } from '../../utils/openExternal'
import { type AttentionTone, type ProviderAttention, providerAttention } from '../../utils/providerAttention'
import { describeRefreshResult } from '../../utils/refreshSummary'

const MAX_VISIBLE = 3
const CLOSE_BUTTON_SELECTOR = '.mantine-Alert-closeButton'
const SETTINGS_KINDS: FailureKind[] = ['auth', 'permission', 'config', 'tls']

interface BannerEntry {
  provider: ProviderSummary
  attention: ProviderAttention
  tone: AttentionTone
  episodeKey: string
  headline: string
}

export const attentionColor = (summary: ProviderSummary, tone: AttentionTone): string =>
  tone === 'danger' ? 'red' : HEALTH_META[tone === 'info' ? 'maintenance' : summary.health.level].color

const openLink = (url: string) => {
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

interface ProviderRecoveryActionsProps {
  providerId: number
  kind: FailureKind | null
}

export function ProviderRecoveryActions({ providerId, kind }: ProviderRecoveryActionsProps) {
  const refresh = useRefreshProvider()

  return (
    <>
      <Button
        size="compact-xs"
        variant="default"
        leftSection={<IconRefresh size={14} aria-hidden />}
        loading={refresh.isPending}
        onClick={() =>
          refresh.mutate(providerId, { onError: (error) => displayErrorNotification(error, "Couldn't refresh") })
        }
      >
        Retry now
      </Button>
      {kind && SETTINGS_KINDS.includes(kind) && (
        <Button
          component={Link}
          to="/settings/providers"
          size="compact-xs"
          variant="default"
          leftSection={<IconSettings size={14} aria-hidden />}
        >
          Open provider settings
        </Button>
      )}
      {refresh.data && (
        <Text size="xs" c="dimmed">
          {describeRefreshResult(refresh.data)}
        </Text>
      )}
    </>
  )
}

interface ProviderAlertProps {
  entry: BannerEntry
  now: number
  onDismiss: () => void
}

function ProviderAlert({ entry, now, onDismiss }: ProviderAlertProps) {
  const { provider, attention, tone, episodeKey, headline } = entry
  const { health } = provider
  const signal = health.signals.find((item) => `${provider.id}:${item.episode_id}` === episodeKey)
  const StatusIcon = signal
    ? (SIGNAL_ICONS[signal.kind] ?? HEALTH_META[tone === 'info' ? 'maintenance' : health.level].icon)
    : IconAlertCircle
  const statusPage = signal?.kind === 'status_page' ? health.status_page : null
  const incidents = signal?.incidents ?? []
  const maintenances = signal?.maintenances.filter((item) => item.in_progress) ?? []
  const affected = signal?.components.filter((item) => item.level !== 'operational').map((item) => item.name) ?? []
  const hasUpdate = incidents.some((item) => item.latest_update)
  const detail =
    attention.detail && !hasUpdate && !attention.hint?.startsWith(attention.detail) ? attention.detail : null
  const dataFrom = provider.last_fetch_status === 'error' ? provider.last_updated : null

  return (
    <Alert
      role="group"
      variant="light"
      color={attentionColor(provider, tone)}
      p="xs"
      icon={<StatusIcon size={16} aria-hidden />}
      title={headline.includes(provider.name) ? headline : `${provider.name}: ${headline}`}
      withCloseButton
      closeButtonLabel={`Dismiss ${provider.name} status`}
      onClose={onDismiss}
    >
      <Stack gap={4}>
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
          <ProviderRecoveryActions providerId={provider.id} kind={attention.kind} />
        </Group>
      </Stack>
    </Alert>
  )
}

interface ProviderHealthBannerProps {
  providerIds?: number[]
}

export function ProviderHealthBanner({ providerIds }: ProviderHealthBannerProps) {
  const { data: providers } = useProviders()
  const now = useNow(30_000)
  const dismissed = useHealthDismissalStore((state) => state.dismissed)
  const dismiss = useHealthDismissalStore((state) => state.dismiss)
  const prune = useHealthDismissalStore((state) => state.prune)
  const [expanded, setExpanded] = useState(false)
  const [announcement, setAnnouncement] = useState('')
  const previousLevels = useRef<Map<number, HealthLevel> | null>(null)
  const containerRef = useRef<HTMLDivElement>(null)

  const entries = useMemo(
    () =>
      providers?.flatMap((provider): BannerEntry[] => {
        const attention = providerAttention(provider)
        const { tone, episodeKey, headline } = attention

        return tone && episodeKey && headline ? [{ provider, attention, tone, episodeKey, headline }] : []
      }),
    [providers]
  )

  useEffect(() => {
    if (entries) {
      prune(entries.map((entry) => entry.episodeKey))
    }
  }, [entries, prune])

  useEffect(() => {
    if (!providers) {
      return
    }

    const previous = previousLevels.current

    previousLevels.current = new Map(providers.map((provider) => [provider.id, provider.health.level]))

    if (!previous) {
      return
    }

    const changes = providers
      .filter((provider) => {
        const before = previous.get(provider.id)

        return before !== undefined && before !== provider.health.level
      })
      .map((provider) => `${provider.name} status: ${HEALTH_META[provider.health.level].label}`)

    if (changes.length > 0) {
      setAnnouncement(changes.join('. '))
    }
  }, [providers])

  const visible = (entries ?? []).filter(
    (entry) => !dismissed.includes(entry.episodeKey) && (!providerIds || providerIds.includes(entry.provider.id))
  )
  const shown = expanded ? visible : visible.slice(0, MAX_VISIBLE)

  const dismissAt = (index: number, episodeKey: string) => {
    const next = containerRef.current?.querySelectorAll<HTMLElement>(CLOSE_BUTTON_SELECTOR)[index + 1]

    const target = next ?? document.querySelector<HTMLElement>('[data-page-heading]')

    target?.focus()
    dismiss(episodeKey)
  }

  return (
    <>
      <VisuallyHidden role="status">{announcement}</VisuallyHidden>
      {visible.length > 0 && (
        <Stack gap="xs" ref={containerRef}>
          {shown.map((entry, index) => (
            <ProviderAlert
              key={entry.episodeKey}
              entry={entry}
              now={now}
              onDismiss={() => dismissAt(index, entry.episodeKey)}
            />
          ))}
          {visible.length > MAX_VISIBLE && (
            <Button
              size="compact-xs"
              variant="subtle"
              color="gray"
              style={{ alignSelf: 'flex-start' }}
              aria-expanded={expanded}
              onClick={() => setExpanded((value) => !value)}
            >
              {expanded ? 'Show fewer' : `+${visible.length - MAX_VISIBLE} more`}
            </Button>
          )}
        </Stack>
      )}
    </>
  )
}
