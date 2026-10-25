import { ActionIcon, Button, Menu, Stack } from '@mantine/core'
import { IconDots, IconEdit, IconPlus, IconStack2, IconTrash } from '@tabler/icons-react'

import { useProviders } from '../../queries/useProvidersQueries'
import { useProviderStore } from '../../stores/providerStore'
import { formatCount } from '../../utils/formatCount'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { IosGroup, IosRow } from '../ios/IosList'
import { SwipeActions } from '../ios/SwipeActions'
import { ProviderAvatar, ProviderStatusLine } from '../layout/Navbar'

import { useConfirmRemoveProvider } from './ProviderModals'
import classes from './ProvidersPage.module.css'

const AVATAR = 30

export function ProvidersPage() {
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
        onRetry={() => refetch()}
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
          <div key={provider.id} className={classes.shell}>
            <SwipeActions
              actions={[
                {
                  key: 'remove',
                  label: 'Remove',
                  tone: 'red',
                  onSelect: () => confirmRemove(provider.id, provider.name),
                },
                {
                  key: 'edit',
                  label: 'Edit',
                  tone: 'blue',
                  primary: true,
                  onSelect: () => requestEditProvider(provider.id),
                },
              ]}
            >
              <IosRow
                icon={<ProviderAvatar provider={provider} size={AVATAR} />}
                title={provider.name}
                subtitle={<ProviderStatusLine provider={provider} />}
                to={`/pipelines?provider=${provider.id}`}
                chevron={false}
              />
              <Menu position="bottom-end" withinPortal>
                <Menu.Target>
                  <ActionIcon
                    className={classes.actions}
                    variant="subtle"
                    color="gray"
                    size={44}
                    aria-label={`Actions for ${provider.name}`}
                  >
                    <IconDots size={20} />
                  </ActionIcon>
                </Menu.Target>
                <Menu.Dropdown>
                  <Menu.Label>{provider.name}</Menu.Label>
                  <Menu.Item leftSection={<IconEdit size={16} />} onClick={() => requestEditProvider(provider.id)}>
                    Edit
                  </Menu.Item>
                  <Menu.Item
                    color="red"
                    leftSection={<IconTrash size={16} />}
                    onClick={() => confirmRemove(provider.id, provider.name)}
                  >
                    Remove
                  </Menu.Item>
                  <ActionSheetCancel />
                </Menu.Dropdown>
              </Menu>
            </SwipeActions>
          </div>
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
