import { Group, Text, Tooltip, UnstyledButton } from '@mantine/core'

import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import type { ProviderSummary } from '../../types'
import { providerAttention } from '../../utils/providerAttention'
import { TABLE_CELL_STYLES } from '../../utils/tableCells'
import { ProviderIcon } from '../atoms/ProviderIcon'

export function ProviderStatusButton({ provider }: { provider: ProviderSummary | undefined }) {
  const open = useNotificationCenterStore((state) => state.open)

  if (!provider) {
    return (
      <Text size={TABLE_CELL_STYLES.text.size} c="dimmed">
        Unknown
      </Text>
    )
  }

  const { tone, headline } = providerAttention(provider)
  const attention = tone && headline ? { tone, label: headline } : undefined

  return (
    <Tooltip label={attention ? `${headline} · View service status` : 'View service status'} openDelay={300}>
      <UnstyledButton
        className="interactive-surface"
        px={4}
        py={2}
        maw="100%"
        aria-label={`View ${provider.name} service status${attention ? `: ${headline}` : ''}`}
        style={{ borderRadius: 'var(--mantine-radius-sm)' }}
        onClick={(event) => {
          event.stopPropagation()
          open({ type: 'provider', providerId: provider.id })
        }}
      >
        <Group gap={TABLE_CELL_STYLES.spacing.gap} wrap="nowrap">
          <ProviderIcon name={provider.name} icon={provider.icon} size={22} attention={attention} />
          <Text size={TABLE_CELL_STYLES.text.size} truncate>
            {provider.name}
          </Text>
        </Group>
      </UnstyledButton>
    </Tooltip>
  )
}
