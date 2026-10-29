import { Anchor, Button, Divider, Group, Modal, Stack, Text } from '@mantine/core'
import { IconExternalLink } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useNow } from '../../hooks/useNow'
import { useProviders } from '../../queries/useProvidersQueries'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { HEALTH_META } from '../../theme/status'
import type { ComponentHealth, Incident, Maintenance, ProviderSummary } from '../../types'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { formatDateTime } from '../../utils/formatDateTime'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { openExternal } from '../../utils/openExternal'
import { fetchHint, providerAttention, scheduleNote } from '../../utils/providerAttention'
import { ProviderIcon } from '../atoms/ProviderIcon'
import { StandardModal } from '../common/StandardModal'
import { IosGroup, IosRow } from '../ios/IosList'
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

  return starts_at ? `Starts ${formatDateTime(starts_at)}` : null
}

function Section({ title, children }: { title: string; children: ReactNode }) {
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
      onClick={() => openLink(url)}
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

const checkedText = (page: StatusPage, now: number): string => {
  if (page.error) {
    return page.checked_at
      ? `${page.error}. Last successful check ${formatRelativeTime(page.checked_at, now)}`
      : page.error
  }

  return page.checked_at ? `Checked ${formatRelativeTime(page.checked_at, now)}` : 'Not checked yet'
}

const connectionState = (provider: ProviderSummary, failureTitle: string | undefined, now: number): string => {
  if (provider.last_fetch_status === 'success') {
    const lastFetch = provider.last_fetch_at ? formatRelativeTime(provider.last_fetch_at, now) : null

    return `Working · last fetch ${lastFetch ?? 'just now'}`
  }

  return provider.last_fetch_status === 'error' ? (failureTitle ?? 'Last fetch failed') : 'Not fetched yet'
}

function ServiceStatus({ provider, now }: { provider: ProviderSummary; now: number }) {
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
          onClick={() => openLink(page.url)}
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

function Controllers({ provider }: { provider: ProviderSummary }) {
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

function Connection({ provider, now, onNavigate }: { provider: ProviderSummary; now: number; onNavigate: () => void }) {
  const attention = providerAttention(provider)
  const connection = provider.health.signals.find((item) => item.kind === 'connection' || item.kind === 'rate_limit')
  const state = connectionState(provider, connection?.title, now)
  const notes = [fetchHint(provider), scheduleNote(provider)].filter(Boolean)

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
      {attention.staleNotes.map((note) => (
        <Text key={note} size="xs" c="dimmed">
          {note}
        </Text>
      ))}
      <Group gap="xs" mt="3xs">
        <ProviderRecoveryActions providerId={provider.id} recovery={attention.recovery} onNavigate={onNavigate} />
      </Group>
    </Section>
  )
}

function healthText(component: ComponentHealth) {
  const healthy = component.level === 'operational'

  return (
    <Text span inherit c={healthy ? 'dimmed' : HEALTH_META[component.level].color}>
      {component.detail ?? HEALTH_META[component.level].label}
    </Text>
  )
}

function ShellComponentRows({ components }: { components: ComponentHealth[] }) {
  return (
    <>
      {components.map((component) => (
        <IosRow key={component.name} title={component.name} value={healthText(component)} />
      ))}
    </>
  )
}

function ShellIncidentRow({ incident, now }: { incident: Incident; now: number }) {
  const { url } = incident
  const updated = formatRelativeTime(incident.updated_at, now)
  const meta = `${HEALTH_META[incident.level].label} · ${incident.phase} · Updated ${updated}`

  return (
    <IosRow
      title={incident.name}
      subtitle={
        <>
          {meta}
          {incident.latest_update && (
            <>
              <br />
              {incident.latest_update}
            </>
          )}
        </>
      }
      chevron={Boolean(url)}
      {...(url ? { onClick: () => openLink(url) } : {})}
    />
  )
}

function ShellMaintenanceRow({ item }: { item: Maintenance }) {
  const { url } = item
  const meta = [item.in_progress ? 'Maintenance in progress' : 'Scheduled maintenance', maintenanceWindow(item)]

  return (
    <IosRow
      title={item.name}
      subtitle={meta.filter(Boolean).join(' · ')}
      chevron={Boolean(url)}
      {...(url ? { onClick: () => openLink(url) } : {})}
    />
  )
}

interface ShellProviderStatusProps {
  provider: ProviderSummary
  now: number
  onNavigate: () => void
}

function ShellProviderStatus({ provider, now, onNavigate }: ShellProviderStatusProps) {
  const attention = providerAttention(provider)
  const page = provider.health.status_page
  const signal = provider.health.signals.find((item) => item.kind === 'status_page')
  const instance = provider.health.signals.find((item) => item.kind === 'instance')
  const connection = provider.health.signals.find((item) => item.kind === 'connection' || item.kind === 'rate_limit')
  const notes = [
    fetchHint(provider),
    scheduleNote(provider),
    provider.last_fetch_status === 'error' && provider.last_updated
      ? `Showing data from ${formatRelativeTime(provider.last_updated, now)}`
      : null,
    ...attention.staleNotes,
  ].filter(Boolean)

  return (
    <>
      {page ? (
        <IosGroup
          header={`${page.name} status`}
          footer={`${checkedText(page, now)}${signal?.evidence === 'stale' ? ' · Last report may be out of date' : ''}`}
        >
          <ShellComponentRows components={page.components} />
          <IosRow
            icon={<IconExternalLink size={18} aria-hidden />}
            title="Open status page"
            chevron
            onClick={() => openLink(page.url)}
          />
        </IosGroup>
      ) : (
        <IosGroup header="Service status" footer="Pipedash reports connection problems.">
          <IosRow title="No public status page" />
        </IosGroup>
      )}

      {signal && signal.incidents.length > 0 && (
        <IosGroup header="Incidents">
          {signal.incidents.map((incident) => (
            <ShellIncidentRow key={incident.id} incident={incident} now={now} />
          ))}
        </IosGroup>
      )}

      {signal && signal.maintenances.length > 0 && (
        <IosGroup header="Maintenance">
          {signal.maintenances.map((item) => (
            <ShellMaintenanceRow key={item.id} item={item} />
          ))}
        </IosGroup>
      )}

      {page && page.other_incidents.length > 0 && (
        <IosGroup
          header="Other incidents"
          footer="Other incidents on this page. They don't affect the services Pipedash uses."
        >
          {page.other_incidents.map((incident) => (
            <ShellIncidentRow key={incident.id} incident={incident} now={now} />
          ))}
        </IosGroup>
      )}

      {instance && (
        <IosGroup header="Controllers">
          <ShellComponentRows components={instance.components} />
        </IosGroup>
      )}

      <IosGroup
        header="Pipedash connection"
        footer={notes.length > 0 ? notes.map((note) => <div key={note}>{note}</div>) : undefined}
      >
        <IosRow title={connectionState(provider, connection?.title, now)} />
        <Group gap="xs" p="sm" style={{ borderTop: '0.5px solid var(--ios-separator)' }}>
          <ProviderRecoveryActions providerId={provider.id} recovery={attention.recovery} onNavigate={onNavigate} />
        </Group>
      </IosGroup>
    </>
  )
}

export function ProviderStatusModal() {
  const selection = useNotificationCenterStore((state) => state.selection)
  const close = useNotificationCenterStore((state) => state.close)
  const { data: providers } = useProviders()
  const now = useNow(30_000)
  const mobileShell = useMobileShell()

  const provider =
    selection?.type === 'provider' ? providers?.find((item) => item.id === selection.providerId) : undefined
  const attention = provider ? providerAttention(provider) : null

  if (mobileShell) {
    return (
      <Modal
        opened={Boolean(provider)}
        onClose={close}
        title={
          provider && (
            <Group gap="sm" wrap="nowrap">
              <ProviderIcon name={provider.name} icon={provider.icon} size={28} />
              {provider.name}
            </Group>
          )
        }
      >
        {provider && (
          <Stack gap={24}>
            <Text size="sm" c="dimmed">
              {attention?.headline ?? 'No problems affecting your pipelines'}
            </Text>
            <ShellProviderStatus provider={provider} now={now} onNavigate={close} />
          </Stack>
        )}
      </Modal>
    )
  }

  return (
    <StandardModal
      opened={Boolean(provider)}
      onClose={close}
      size="lg"
      title={
        provider && (
          <Group gap="sm" wrap="nowrap">
            <ProviderIcon name={provider.name} icon={provider.icon} size={28} />
            <Stack gap={0}>
              <Text fw={600}>{provider.name}</Text>
              <Text size="xs" c="dimmed">
                {attention?.headline ?? 'No problems affecting your pipelines'}
              </Text>
            </Stack>
          </Group>
        )
      }
    >
      {provider && (
        <Stack gap="md">
          <Section
            title={provider.health.status_page ? `${provider.health.status_page.name} status` : 'Service status'}
          >
            <ServiceStatus provider={provider} now={now} />
          </Section>
          <Controllers provider={provider} />
          <Connection provider={provider} now={now} onNavigate={close} />
        </Stack>
      )}
    </StandardModal>
  )
}
