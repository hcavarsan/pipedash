import { Anchor, Button, Divider, Group, Stack, Text } from '@mantine/core'
import { IconExternalLink } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { ProviderRecoveryActions } from '../../../components/common/ProviderRecoveryActions'
import { HEALTH_META } from '../../../theme/status'
import type { ComponentHealth, Incident, Maintenance } from '../../../types/health'
import type { ProviderSummary } from '../../../types/providers'
import { formatDateTime } from '../../../utils/formatDateTime'
import { formatRelativeTime } from '../../../utils/formatRelativeTime'
import { connectionNotes, staleNotes } from '../lib/attentionNotes'
import { openNotificationLink } from '../lib/openNotificationLink'

export const maintenanceWindow = ({ starts_at, ends_at }: Maintenance): string | null => {
  if (starts_at && ends_at) {
    return `${formatDateTime(starts_at)} – ${formatDateTime(ends_at)}`
  }

  if (ends_at) {
    return `Until ${formatDateTime(ends_at)}`
  }

  return starts_at ? `Starts ${formatDateTime(starts_at)}` : null
}

export function Section({ title, children }: { title: string; children: ReactNode }): ReactNode {
  return (
    <Stack gap="2xs">
      <Divider label={title} labelPosition="left" />
      {children}
    </Stack>
  )
}

function LinkedName({ name, url, source }: { name: string; url: string | null; source: string }) {
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
      c="inherit"
      underline="always"
      aria-label={`View ${name} on ${source}`}
      onClick={() => openNotificationLink(url)}
    >
      {name}
    </Anchor>
  )
}

function ComponentRow({ component }: { component: ComponentHealth }) {
  const healthy = component.level === 'operational'

  return (
    <Group justify="space-between" gap="md" wrap="nowrap">
      <Text size="sm" {...(healthy ? { c: 'dimmed' } : { fw: 500 })} truncate>
        {component.name}
      </Text>
      <Text size="xs" c="dimmed" style={{ flexShrink: 0 }}>
        {component.detail ?? HEALTH_META[component.level].label}
      </Text>
    </Group>
  )
}

function IncidentItem({ incident, source, now }: { incident: Incident; source: string; now: number }) {
  return (
    <Stack gap="4xs">
      <LinkedName name={incident.name} url={incident.url} source={source} />
      <Text size="xs" c="dimmed">
        {HEALTH_META[incident.level].label} · {incident.phase} · Updated {formatRelativeTime(incident.updated_at, now)}
      </Text>
      {incident.latest_update && <Text size="sm">{incident.latest_update}</Text>}
    </Stack>
  )
}

type StatusPage = NonNullable<ProviderSummary['health']['status_page']>

export const checkedText = (page: StatusPage, now: number): string => {
  if (page.error) {
    return page.checked_at
      ? `${page.error}. Last successful check ${formatRelativeTime(page.checked_at, now)}`
      : page.error
  }

  return page.checked_at ? `Checked ${formatRelativeTime(page.checked_at, now)}` : 'Not checked yet'
}

export const connectionState = (provider: ProviderSummary, failureTitle: string | undefined, now: number): string => {
  if (provider.last_fetch_status === 'success') {
    const lastFetch = provider.last_fetch_at ? formatRelativeTime(provider.last_fetch_at, now) : null

    return `Working · last fetch ${lastFetch ?? 'just now'}`
  }

  return provider.last_fetch_status === 'error' ? (failureTitle ?? 'Last fetch failed') : 'Not fetched yet'
}

export function ServiceStatus({ provider, now }: { provider: ProviderSummary; now: number }): ReactNode {
  const page = provider.health.status_page
  const signal = provider.health.signals.find((item) => item.kind === 'status_page')

  if (!page) {
    return (
      <Text size="sm" c="dimmed">
        No public status page. Pipedash reports connection problems.
      </Text>
    )
  }

  const checked = checkedText(page, now)

  return (
    <Stack gap="sm">
      <Group justify="space-between" gap="sm">
        <Text size="xs" c="dimmed">
          {checked}
          {signal?.evidence === 'stale' ? ' · Last report may be out of date' : ''}
        </Text>
        <Button
          size="compact-xs"
          variant="subtle"
          color="gray"
          leftSection={<IconExternalLink size={14} aria-hidden />}
          onClick={() => openNotificationLink(page.url)}
        >
          Open status page
        </Button>
      </Group>

      {page.components.length > 0 && (
        <Stack gap="3xs">
          {page.components.map((component) => (
            <ComponentRow key={component.name} component={component} />
          ))}
        </Stack>
      )}

      {signal?.incidents.map((incident) => (
        <IncidentItem key={incident.id} incident={incident} source={page.name} now={now} />
      ))}

      {signal?.maintenances.map((item) => (
        <Stack key={item.id} gap="4xs">
          <LinkedName name={item.name} url={item.url} source={page.name} />
          <Text size="xs" c="dimmed">
            {[item.in_progress ? 'Maintenance in progress' : 'Scheduled maintenance', maintenanceWindow(item)]
              .filter(Boolean)
              .join(' · ')}
          </Text>
        </Stack>
      ))}

      {page.other_incidents.length > 0 && (
        <Stack gap="2xs">
          <Text size="xs" c="dimmed">
            Other incidents on this page. They don't affect the services Pipedash uses.
          </Text>
          {page.other_incidents.map((incident) => (
            <IncidentItem key={incident.id} incident={incident} source={page.name} now={now} />
          ))}
        </Stack>
      )}
    </Stack>
  )
}

export function Controllers({ provider }: { provider: ProviderSummary }): ReactNode {
  const signal = provider.health.signals.find((item) => item.kind === 'instance')

  if (!signal) {
    return null
  }

  return (
    <Section title="Controllers">
      {signal.components.map((component) => (
        <ComponentRow key={component.name} component={component} />
      ))}
    </Section>
  )
}

export function Connection({
  provider,
  now,
  onNavigate,
}: {
  provider: ProviderSummary
  now: number
  onNavigate: () => void
}): ReactNode {
  const connection = provider.health.signals.find((item) => item.kind === 'connection' || item.kind === 'rate_limit')
  const state = connectionState(provider, connection?.title, now)
  const notes = connectionNotes(provider)

  return (
    <Section title="Pipedash connection">
      <Text size="sm" fw={provider.last_fetch_status === 'error' ? 500 : undefined}>
        {state}
      </Text>
      {notes.length > 0 && (
        <Text size="sm" c="dimmed">
          {notes.join(' ')}
        </Text>
      )}
      {provider.last_fetch_status === 'error' && provider.last_updated && (
        <Text size="xs" c="dimmed">
          Showing data from {formatRelativeTime(provider.last_updated, now)}
        </Text>
      )}
      {staleNotes(provider).map((note) => (
        <Text key={note} size="xs" c="dimmed">
          {note}
        </Text>
      ))}
      <Group gap="xs" mt="3xs">
        <ProviderRecoveryActions
          providerId={provider.id}
          recovery={provider.attention.recovery}
          onNavigate={onNavigate}
        />
      </Group>
    </Section>
  )
}
