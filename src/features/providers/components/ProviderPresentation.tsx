import { Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { ProviderIcon } from '../../../components/atoms/ProviderIcon'
import type { ProviderSummary } from '../../../types/domain'
import { providerBadge } from '../../../utils/fetchInfo'
import { formatCount } from '../../../utils/formatCount'
import { formatTime } from '../../../utils/formatDateTime'

export const providerStatus = (provider: ProviderSummary): { text: string; failed: boolean } => {
  if (provider.paused_until) {
    return { text: `Paused until ${formatTime(provider.paused_until)}`, failed: false }
  }
  if (provider.last_fetch_status === 'error') {
    return { text: 'Fetch failed', failed: true }
  }

  return { text: formatCount(provider.pipeline_count, 'pipeline'), failed: false }
}

export const ProviderAvatar = ({ provider, size }: { provider: ProviderSummary; size: number }): ReactNode => (
  <ProviderIcon name={provider.name} icon={provider.icon} size={size} badge={providerBadge(provider, Date.now())} />
)

export const ProviderStatusLine = ({ provider }: { provider: ProviderSummary }): ReactNode => {
  const { text, failed } = providerStatus(provider)

  return (
    <Text size="xs" c={failed ? 'red' : 'dimmed'} truncate>
      {text}
    </Text>
  )
}
