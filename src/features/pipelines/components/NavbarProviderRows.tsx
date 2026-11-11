import { ActionIcon, Group, Tooltip } from '@mantine/core'
import { IconEdit } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import type { ProviderSummary } from '../../../types/providers'
import { ProviderAvatar, ProviderStatusLine } from '../../providers/components/ProviderPresentation'
import { useProviderNavigation } from '../../providers/queries/useProviderNavigation'
import { ICON_TILE, type MenuProps, NavbarTreeRow } from './NavbarTreeRow'

interface ProviderRowsProps {
  providers: ProviderSummary[]
  visibleProviders: ProviderSummary[]
  providerId: string | null
  loading: boolean
  error: unknown
  retrying: boolean
  onRetry: () => void
  providerMenu: (provider: ProviderSummary) => MenuProps
  onEditProvider: (id: number) => void
}

function EditProviderAction({ name, onEdit }: { name: string; onEdit: () => void }): ReactNode {
  return (
    <Group gap="4xs" wrap="nowrap" className="nav-item-actions">
      <Tooltip label={`Edit ${name}`}>
        <ActionIcon size="sm" aria-label={`Edit ${name}`} onClick={onEdit}>
          <IconEdit size={14} aria-hidden />
        </ActionIcon>
      </Tooltip>
    </Group>
  )
}

export const ProviderRows = ({
  providers,
  visibleProviders,
  providerId,
  loading,
  error,
  retrying,
  onRetry,
  providerMenu,
  onEditProvider,
}: ProviderRowsProps): ReactNode => {
  const openProvider = useProviderNavigation()

  if (loading) {
    return <LoadingState message="Loading providers" />
  }

  const alert = error ? (
    <ErrorState
      title="Couldn't load providers"
      error={error}
      hint="Check your connection and provider credentials."
      onRetry={onRetry}
      retrying={retrying}
    />
  ) : null

  if (providers.length === 0) {
    return alert ?? <EmptyState title="No providers yet" description="Add a provider to see its pipelines here." />
  }

  return (
    <>
      {alert}
      {visibleProviders.map((provider) => (
        <NavbarTreeRow
          key={provider.id}
          depth={1}
          label={provider.name}
          icon={<ProviderAvatar provider={provider} size={ICON_TILE.list} />}
          description={<ProviderStatusLine provider={provider} />}
          active={providerId === String(provider.id)}
          menu={providerMenu(provider)}
          to={`/pipelines?provider=${provider.id}`}
          onClick={(event) => openProvider(provider, event)}
          actions={<EditProviderAction name={provider.name} onEdit={() => onEditProvider(provider.id)} />}
        />
      ))}
    </>
  )
}
