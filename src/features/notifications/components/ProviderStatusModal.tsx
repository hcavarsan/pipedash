import { Group, Modal, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { ProviderIcon } from '../../../components/atoms/ProviderIcon'
import { StandardModal } from '../../../components/common/StandardModal'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import { useProviders } from '../../../queries/useProvidersQueries'
import { useNotificationCenterStore } from '../../../stores/notificationCenterStore'
import type { ProviderSummary } from '../../../types/providers'
import { Connection, Controllers, Section, ServiceStatus } from './ProviderServiceStatus'
import { ShellProviderStatus } from './ShellProviderStatus'

const CONNECTION_SUMMARY: Record<ProviderSummary['last_fetch_status'], string> = {
  success: 'connection working',
  error: 'last fetch failed',
  never: 'not fetched yet',
}

function statusSummary(provider: ProviderSummary, headline: string | null | undefined): string {
  return `${provider.name}: ${headline ?? 'No problems affecting your pipelines'}. ${CONNECTION_SUMMARY[provider.last_fetch_status]}.`
}

export function ProviderStatusModal(): ReactNode {
  const selection = useNotificationCenterStore((state) => state.selection)
  const close = useNotificationCenterStore((state) => state.close)
  const { data: providers } = useProviders()
  const now = useNow(30_000)
  const mobileShell = useMobileShell()

  const provider =
    selection?.type === 'provider' ? providers?.find((item) => item.id === selection.providerId) : undefined
  const attention = provider?.attention ?? null
  const headline = attention?.headline ?? 'No problems affecting your pipelines'

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
            <LiveStatus message={statusSummary(provider, attention?.headline)} />
            <Text size="sm" c="dimmed">
              {headline}
            </Text>
            <ShellProviderStatus provider={provider} now={now} onNavigate={close} />
          </Stack>
        )}
      </Modal>
    )
  }

  const title = provider && (
    <Group gap="sm" wrap="nowrap">
      <ProviderIcon name={provider.name} icon={provider.icon} size={28} />
      <Stack gap={0}>
        <Text fw={600}>{provider.name}</Text>
        <Text size="xs" c="dimmed">
          {headline}
        </Text>
      </Stack>
    </Group>
  )
  const statusPage = provider?.health.status_page
  const serviceTitle = statusPage ? `${statusPage.name} status` : 'Service status'

  return (
    <StandardModal opened={Boolean(provider)} onClose={close} size="lg" title={title}>
      {provider && (
        <Stack gap="md">
          <LiveStatus message={statusSummary(provider, attention?.headline)} />
          <Connection provider={provider} now={now} onNavigate={close} />
          <Section title={serviceTitle}>
            <ServiceStatus provider={provider} now={now} />
          </Section>
          <Controllers provider={provider} />
        </Stack>
      )}
    </StandardModal>
  )
}
