import { ActionIcon, Button, Menu, Stack } from '@mantine/core'
import { IconDots, IconEdit, IconPlus, IconStack2, IconTrash } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { SwipeActions } from '../../../components/ios/SwipeActions'
import { useRowMenuLongPress } from '../../../hooks/useLongPress'
import { useProviders } from '../../../queries/useProvidersQueries'
import { useProviderStore } from '../../../stores/providerStore'
import type { ProviderSummary } from '../../../types/domain'
import { formatCount } from '../../../utils/formatCount'
import { useConfirmRemoveProvider } from './ProviderModals'
import { ProviderAvatar, ProviderStatusLine } from './ProviderPresentation'
import classes from './ProvidersPage.module.css'

const AVATAR = 30

interface ProviderRowProps {
  provider: ProviderSummary
  onEdit: () => void
  onRemove: () => void
}

function ProviderRow({ provider, onEdit, onRemove }: ProviderRowProps) {
  const pressRef = useRowMenuLongPress()

  return (
    <div ref={pressRef} className={classes.shell}>
      <SwipeActions
        actions={[
          { key: 'remove', label: 'Remove', tone: 'red', onSelect: onRemove },
          { key: 'edit', label: 'Edit', tone: 'blue', primary: true, onSelect: onEdit },
        ]}
      >
        <IosRow
          icon={<ProviderAvatar provider={provider} size={AVATAR} />}
          title={provider.name}
          subtitle={<ProviderStatusLine provider={provider} />}
          to={`/pipelines?provider=${provider.id}`}
        />
        <Menu position="bottom-end" withinPortal>
          <Menu.Target>
            <ActionIcon className={classes.actions} size={44} data-row-menu aria-label={`Actions for ${provider.name}`}>
              <IconDots size={20} aria-hidden />
            </ActionIcon>
          </Menu.Target>
          <Menu.Dropdown>
            <Menu.Label>{provider.name}</Menu.Label>
            <Menu.Item leftSection={<IconEdit size={16} aria-hidden />} onClick={onEdit}>
              Edit
            </Menu.Item>
            <Menu.Item color="red" leftSection={<IconTrash size={16} aria-hidden />} onClick={onRemove}>
              Remove
            </Menu.Item>
            <ActionSheetCancel />
          </Menu.Dropdown>
        </Menu>
      </SwipeActions>
    </div>
  )
}

export function ProvidersPage(): ReactNode {
  const { data: providers = [], isLoading, error, refetch, isRefetching } = useProviders()
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const requestEditProvider = useProviderStore((s) => s.requestEditProvider)
  const confirmRemove = useConfirmRemoveProvider()
  const totalPipelines = providers.reduce((sum, provider) => sum + provider.pipeline_count, 0)

  if (isLoading) {
    return <LoadingState message="Loading providers" />
  }

  if (error) {
    return (
      <ErrorState
        title="Couldn't load providers"
        error={error}
        hint="Check your connection and provider credentials."
        onRetry={() => void refetch()}
        retrying={isRefetching}
      />
    )
  }

  if (providers.length === 0) {
    return (
      <EmptyState
        title="No providers yet"
        description="Add a provider to see its pipelines here."
        action={<Button onClick={openAddProviderModal}>Add provider</Button>}
      />
    )
  }

  return (
    <Stack gap="md" pb="md">
      <IosGroup>
        <IosRow
          icon={<IconStack2 size={18} />}
          title="All providers"
          subtitle={formatCount(totalPipelines, 'pipeline')}
          to="/pipelines"
        />
      </IosGroup>

      <IosGroup>
        {providers.map((provider) => (
          <ProviderRow
            key={provider.id}
            provider={provider}
            onEdit={() => requestEditProvider(provider.id)}
            onRemove={() => confirmRemove(provider.id, provider.name)}
          />
        ))}
      </IosGroup>

      <IosGroup>
        <IosRow
          icon={<IconPlus size={18} />}
          title={<span className={classes.addTitle}>Add provider</span>}
          onClick={openAddProviderModal}
        />
      </IosGroup>
    </Stack>
  )
}
