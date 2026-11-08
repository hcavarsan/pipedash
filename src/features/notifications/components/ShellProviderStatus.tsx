import { Group, Text } from '@mantine/core'
import { IconExternalLink } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { ProviderRecoveryActions } from '../../../components/common/ProviderRecoveryActions'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { HEALTH_META } from '../../../theme/status'
import type { ComponentHealth, Incident, Maintenance } from '../../../types/health'
import type { ProviderSummary } from '../../../types/providers'
import { formatRelativeTime } from '../../../utils/formatRelativeTime'
import { connectionNotes, staleNotes } from '../lib/attentionNotes'
import { openNotificationLink } from '../lib/openNotificationLink'
import { checkedText, connectionState, maintenanceWindow } from './ProviderServiceStatus'

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
      {...(url ? { onClick: () => openNotificationLink(url) } : {})}
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
      {...(url ? { onClick: () => openNotificationLink(url) } : {})}
    />
  )
}

export function ShellProviderStatus(p: { provider: ProviderSummary; now: number; onNavigate: () => void }): ReactNode {
  const { provider, now, onNavigate } = p
  const page = provider.health.status_page
  const signal = provider.health.signals.find((item) => item.kind === 'status_page')
  const instance = provider.health.signals.find((item) => item.kind === 'instance')
  const connection = provider.health.signals.find((item) => item.kind === 'connection' || item.kind === 'rate_limit')
  const notes = [
    ...connectionNotes(provider),
    provider.last_fetch_status === 'error' && provider.last_updated
      ? `Showing data from ${formatRelativeTime(provider.last_updated, now)}`
      : null,
    ...staleNotes(provider),
  ].filter(Boolean)

  return (
    <>
      <IosGroup
        header="Pipedash connection"
        footer={notes.length > 0 ? notes.map((note) => <div key={note}>{note}</div>) : undefined}
      >
        <IosRow title={connectionState(provider, connection?.title, now)} />
        <Group gap="xs" p="sm" style={{ borderTop: '0.5px solid var(--ios-separator)' }}>
          <ProviderRecoveryActions
            providerId={provider.id}
            recovery={provider.attention.recovery}
            onNavigate={onNavigate}
          />
        </Group>
      </IosGroup>
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
            onClick={() => openNotificationLink(page.url)}
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
    </>
  )
}
