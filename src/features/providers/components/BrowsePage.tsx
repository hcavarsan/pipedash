import { ActionIcon, Menu, Stack } from '@mantine/core'
import {
  IconActivity,
  IconDots,
  IconEdit,
  IconFolder,
  IconLogin,
  IconPlus,
  IconStack2,
  IconTopologyStar3,
  IconTrash,
} from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { AccentText } from '../../../components/ios/mobileParts'
import { useProviders } from '../../../queries/useProvidersQueries'
import { useNotificationCenterStore } from '../../../stores/notificationCenterStore'
import { useProviderStore } from '../../../stores/providerStore'
import type { ProviderSummary } from '../../../types/providers'
import type { PipelineView } from '../../../types/views'
import { useNotifyItems } from '../../notifications/hooks/useNotifyItems'
import { usePipelineViews } from '../../pipelines/queries/usePipelineViewQueries'
import { useProviderNavigation } from '../queries/useProviderNavigation'
import { useProviderSignIn } from '../queries/useProviderSignIn'
import { useConfirmRemoveProvider } from './ProviderModals'
import { ProviderAvatar, ProviderStatusLine } from './ProviderPresentation'

const AVATAR = 30

interface ProviderRowProps {
  provider: ProviderSummary
  onEdit: () => void
  onRemove: () => void
}

function ProviderRow({ provider, onEdit, onRemove }: ProviderRowProps) {
  const openProvider = useProviderNavigation()
  const signIn = useProviderSignIn()
  const openStatus = useNotificationCenterStore((state) => state.open)
  const mute = useNotifyItems().providerItem(provider)

  const items = [
    provider.recovery === 'sign_in' && (
      <Menu.Item key="sign-in" leftSection={<IconLogin size={16} aria-hidden />} onClick={() => signIn(provider.id)}>
        Sign in
      </Menu.Item>
    ),
    <Menu.Item
      key="status"
      leftSection={<IconActivity size={16} aria-hidden />}
      onClick={() => openStatus({ type: 'provider', providerId: provider.id })}
    >
      Provider status
    </Menu.Item>,
    <Menu.Item key="mute" leftSection={mute.icon} onClick={mute.onClick}>
      {mute.label}
    </Menu.Item>,
    <Menu.Item key="edit" leftSection={<IconEdit size={16} aria-hidden />} onClick={onEdit}>
      Edit
    </Menu.Item>,
    <Menu.Item key="remove" color="red" leftSection={<IconTrash size={16} aria-hidden />} onClick={onRemove}>
      Remove
    </Menu.Item>,
  ]

  return (
    <IosRow
      icon={<ProviderAvatar provider={provider} size={AVATAR} />}
      title={provider.name}
      subtitle={<ProviderStatusLine provider={provider} />}
      chevron
      longPressMenu
      onClick={() => openProvider(provider)}
      swipeActions={[
        { key: 'remove', label: 'Remove', tone: 'red', onSelect: onRemove },
        { key: 'edit', label: 'Edit', tone: 'blue', primary: true, onSelect: onEdit },
      ]}
      menu={
        <Menu position="bottom-end" withinPortal>
          <Menu.Target>
            <ActionIcon size={44} data-row-menu aria-label={`Actions for ${provider.name}`}>
              <IconDots size={20} aria-hidden />
            </ActionIcon>
          </Menu.Target>
          <Menu.Dropdown>
            <Menu.Label>{provider.name}</Menu.Label>
            {items}
            <ActionSheetCancel />
          </Menu.Dropdown>
        </Menu>
      }
    />
  )
}

const browsable = (view: PipelineView) =>
  view.parent === null && (view.kind === 'all' || view.kind === 'custom' || view.kind === 'config')

function ViewRow({ view }: { view: PipelineView }): ReactNode {
  const all = view.kind === 'all'

  return (
    <IosRow
      icon={all ? <IconStack2 size={16} /> : <IconFolder size={16} />}
      title={all ? 'All pipelines' : view.name}
      value={String(view.count)}
      to={all ? '/pipelines' : `/pipelines?view=${encodeURIComponent(view.id)}`}
    />
  )
}

export function BrowsePage(): ReactNode {
  const { data: loaded, isLoading, error, refetch, isRefetching } = useProviders()
  const providers = loaded ?? []
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const requestEditProvider = useProviderStore((s) => s.requestEditProvider)
  const confirmRemove = useConfirmRemoveProvider()
  const {
    data: views,
    isLoading: loadingViews,
    error: viewsError,
    refetch: refetchViews,
    isFetching: fetchingViews,
  } = usePipelineViews()

  if (isLoading) {
    return <LoadingState message="Loading providers…" />
  }

  if (error && loaded === undefined) {
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

  const browsableViews = (views ?? []).filter(browsable)

  return (
    <Stack gap="md" pb="md">
      {error && (
        <ErrorState
          title="Couldn't refresh providers"
          error={error}
          onRetry={() => void refetch()}
          retrying={isRefetching}
        />
      )}
      {viewsError && (
        <ErrorState
          title="Couldn't load views"
          error={viewsError}
          onRetry={() => void refetchViews()}
          retrying={fetchingViews}
        />
      )}
      {loadingViews && <LoadingState variant="compact" message="Loading views…" />}
      <IosGroup>
        <IosRow icon={<IconTopologyStar3 size={16} />} title="Applications" to="/applications" />
      </IosGroup>
      {browsableViews.length > 0 && (
        <IosGroup header="Views">
          {browsableViews.map((view) => (
            <ViewRow key={view.id} view={view} />
          ))}
        </IosGroup>
      )}

      <IosGroup
        header="Providers"
        footer={providers.length === 0 ? 'Add a provider to see its pipelines here.' : undefined}
      >
        {providers.map((provider) => (
          <ProviderRow
            key={provider.id}
            provider={provider}
            onEdit={() => requestEditProvider(provider.id)}
            onRemove={() => confirmRemove(provider.id, provider.name)}
          />
        ))}
        <IosRow
          icon={<IconPlus size={16} />}
          title={<AccentText>Add provider</AccentText>}
          onClick={openAddProviderModal}
        />
      </IosGroup>
    </Stack>
  )
}
