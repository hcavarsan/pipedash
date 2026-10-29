import { Box, Button, Stack } from '@mantine/core'
import { type ReactNode, useState } from 'react'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import { useProviders } from '../../../queries/useProvidersQueries'
import { useStorageConfig } from '../../../queries/useStorageQueries'
import { useProviderStore } from '../../../stores/providerStore'
import { EmptyState } from '../../common/EmptyState'
import { ErrorState } from '../../common/ErrorState'
import { LoadingState } from '../../common/LoadingState'
import { ProviderSettingsCard } from './ProviderSettingsCard'
import { ProviderTransferActions } from './ProviderTransferActions'

export const ProvidersSection = (): ReactNode => {
  const mobileShell = useMobileShell()
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const {
    data: providers = [],
    isLoading: loading,
    error: providersError,
    refetch: refetchProviders,
    isRefetching: isRefetchingProviders,
  } = useProviders()
  const now = useNow(30_000)
  const statusPagesEnabled = useStorageConfig().data?.config.general.status_pages_enabled !== false
  const [editingId, setEditingId] = useState<number | null>(null)

  const list = loading ? (
    <LoadingState message="Loading providers..." />
  ) : providersError ? (
    <ErrorState
      title="Couldn't load providers"
      error={providersError}
      onRetry={() => refetchProviders()}
      retrying={isRefetchingProviders}
    />
  ) : providers.length === 0 ? (
    <EmptyState
      title="No providers configured"
      description="Add a CI/CD provider to start monitoring pipelines."
      action={
        <Button size="sm" onClick={openAddProviderModal}>
          Add provider
        </Button>
      }
    />
  ) : (
    <Stack gap={mobileShell ? 'lg' : 'md'}>
      {providers.map((provider) => (
        <ProviderSettingsCard
          key={provider.id}
          provider={provider}
          statusPagesEnabled={statusPagesEnabled}
          now={now}
          editing={editingId === provider.id}
          onStartEditing={() => setEditingId(provider.id)}
          onFinishEditing={() => setEditingId(null)}
        />
      ))}
    </Stack>
  )

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        {list}
        <ProviderTransferActions />
      </Stack>
    )
  }

  return <Box>{list}</Box>
}
