import { Box, Button, Stack } from '@mantine/core'
import { type ReactNode, useState } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import { useProviders } from '../../../queries/useProvidersQueries'
import { useStorageConfig } from '../../../queries/useStorageQueries'
import { useProviderStore } from '../../../stores/providerStore'
import { ProviderSettingsCard } from './ProviderSettingsCard'
import { ProviderTransferActions } from './ProviderTransferActions'

export const ProvidersSection = (): ReactNode => {
  const mobileShell = useMobileShell()
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const {
    data: loaded,
    isLoading: loading,
    error: providersError,
    refetch: refetchProviders,
    isRefetching: isRefetchingProviders,
  } = useProviders()
  const now = useNow(30_000)
  const statusPagesEnabled = useStorageConfig().data?.config.general.status_pages_enabled !== false
  const [editingId, setEditingId] = useState<number | null>(null)

  const providers = loaded ?? []
  const retry = providersError ? (
    <ErrorState
      title="Couldn't load providers"
      error={providersError}
      onRetry={() => refetchProviders()}
      retrying={isRefetchingProviders}
    />
  ) : null
  let list: ReactNode

  if (loading) {
    list = <LoadingState message="Loading providers…" />
  } else if (providersError && loaded === undefined) {
    list = retry
  } else if (providers.length === 0) {
    list = (
      <Stack gap="md">
        {retry}
        <EmptyState
          title="No providers configured"
          description="Add a CI/CD provider to start monitoring pipelines."
          action={
            <Button size="sm" onClick={openAddProviderModal}>
              Add provider
            </Button>
          }
        />
      </Stack>
    )
  } else {
    list = (
      <Stack gap={mobileShell ? 'lg' : 'md'}>
        {retry}
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
  }

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
