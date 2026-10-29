import { Button, Group, Stack, Text } from '@mantine/core'
import { IconExternalLink } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { HEALTH_META } from '../../../theme/status'
import type { ProviderSummary } from '../../../types'
import { formatDateTime } from '../../../utils/formatDateTime'
import { formatRelativeTime } from '../../../utils/formatRelativeTime'
import { openExternal } from '../../../utils/openExternal'
import { IosRow } from '../../ios/IosList'

const CONNECTION_NOTE = 'Pipedash reports connection problems.'

interface ProviderStatusProps {
  provider: ProviderSummary
  statusPagesEnabled: boolean
  now: number
}

type Maintenance = ProviderSummary['health']['signals'][number]['maintenances'][number]

interface StatusDetails {
  page: ProviderSummary['health']['status_page']
  upcoming: Array<{ id: Maintenance['id']; name: Maintenance['name']; at: string }>
  summary: string | null
}

export const getStatusDetails = (
  provider: ProviderSummary,
  statusPagesEnabled: boolean,
  now: number
): StatusDetails => {
  const { status_page: page, signals, level } = provider.health
  const upcoming = statusPagesEnabled
    ? signals
        .filter((signal) => signal.evidence === 'fresh')
        .flatMap((signal) => signal.maintenances)
        .flatMap((item) =>
          !item.in_progress && item.starts_at ? [{ id: item.id, name: item.name, at: item.starts_at }] : []
        )
    : []
  const reportedBy = signals.find((signal) => signal.kind === 'status_page')?.reported_by ?? page?.name
  const summary = page
    ? [
        reportedBy,
        HEALTH_META[level].label,
        !page.error && page.checked_at && `checked ${formatRelativeTime(page.checked_at, now)}`,
      ]
        .filter(Boolean)
        .join(' · ')
    : null

  return { page, upcoming, summary }
}

export const ProviderStatusInfo = ({ provider, statusPagesEnabled, now }: ProviderStatusProps): ReactNode => {
  const { page, upcoming, summary } = getStatusDetails(provider, statusPagesEnabled, now)

  return (
    <Stack gap="3xs">
      <Text size="xs" c="dimmed">
        Status
      </Text>
      {statusPagesEnabled && page ? (
        <Group gap="xs" wrap="wrap">
          <Text size="sm">{summary}</Text>
          <Button
            size="compact-xs"
            variant="subtle"
            color="gray"
            rightSection={<IconExternalLink size={14} aria-hidden />}
            aria-label={`Open ${page.name} status page`}
            onClick={() => {
              void openExternal(page.url)
            }}
          >
            Open status page
          </Button>
        </Group>
      ) : (
        <Text size="sm" c="dimmed">
          {statusPagesEnabled ? `No public status page. ${CONNECTION_NOTE}` : CONNECTION_NOTE}
        </Text>
      )}
      {statusPagesEnabled && page?.error && (
        <Text size="xs" c="orange">
          {page.error}
          {page.checked_at && ` · last successful check ${formatDateTime(page.checked_at)}`}
        </Text>
      )}
      {upcoming.map((item) => (
        <Text key={item.id} size="xs" c="dimmed">
          Maintenance: {item.name} · {formatDateTime(item.at)}
        </Text>
      ))}
    </Stack>
  )
}

export const ProviderStatusRows = ({ provider, statusPagesEnabled, now }: ProviderStatusProps): ReactNode => {
  const { page, upcoming, summary } = getStatusDetails(provider, statusPagesEnabled, now)
  const note = statusPagesEnabled ? `No public status page. ${CONNECTION_NOTE}` : CONNECTION_NOTE

  return (
    <>
      <IosRow title="Status" subtitle={statusPagesEnabled && page ? summary : note} />
      {statusPagesEnabled && page?.error && (
        <IosRow
          title="Status page check failed"
          subtitle={
            <Text span inherit c="orange">
              {page.error}
              {page.checked_at && ` · last successful check ${formatDateTime(page.checked_at)}`}
            </Text>
          }
        />
      )}
      {upcoming.map((item) => (
        <IosRow key={item.id} title="Maintenance" subtitle={`${item.name} · ${formatDateTime(item.at)}`} />
      ))}
      {statusPagesEnabled && page && (
        <IosRow
          title="Open status page"
          ariaLabel={`Open ${page.name} status page`}
          accessory={<IconExternalLink size={18} aria-hidden />}
          onClick={() => {
            void openExternal(page.url)
          }}
        />
      )}
    </>
  )
}
