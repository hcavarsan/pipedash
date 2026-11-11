import { Card, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { QueryBoundary } from '../../../components/common/QueryBoundary'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useProviders } from '../../../queries/useProvidersQueries'
import { PREFERENCE_KINDS, useNotificationStore } from '../../../stores/notificationStore'
import { type PipelinePreference, usePipelinePreferences } from '../hooks/usePipelinePreferences'
import { PipelinePreferenceRow } from './PipelinePreferenceRow'
import { KindSwitch, ProviderSwitch } from './PreferenceSwitches'

const KINDS_NOTE =
  'Applies to notifications and the notification center on this device only. Each phone, computer and browser keeps its own choices.'
const PROVIDERS_NOTE = 'Turn off a provider to stop all of its notifications, except for pipelines you watch.'
const MUTED_EMPTY = 'To mute a pipeline, open its menu and choose Mute notifications.'
const WATCHED_EMPTY =
  'To watch a pipeline, open its menu and choose Watch all runs, or Notify when this run finishes for a single run. Watched pipelines notify you about every finished run, even when its alert type or provider is turned off.'

interface NotificationPreferencesSectionProps {
  hostNote?: string | undefined
}

function SectionHeading({ title, description }: { title: string; description: string }): ReactNode {
  return (
    <Stack gap={0}>
      <Text size="sm" fw={600}>
        {title}
      </Text>
      <Text size="xs" c="dimmed">
        {description}
      </Text>
    </Stack>
  )
}

function NoneRow(): ReactNode {
  return (
    <IosRow
      title={
        <Text span inherit c="dimmed">
          None
        </Text>
      }
    />
  )
}

export function NotificationPreferencesSection({ hostNote }: NotificationPreferencesSectionProps): ReactNode {
  const mobileShell = useMobileShell()
  const preferences = useNotificationStore((state) => state.preferences)
  const providersQuery = useProviders()
  const providers = providersQuery.data ?? []
  const { muted, watching, lookup } = usePipelinePreferences()
  const providersState = (
    <QueryBoundary query={providersQuery} loading="Loading providers…" failure="Couldn't load providers">
      {() => null}
    </QueryBoundary>
  )
  const lookupFailure = lookup.isError && (
    <ErrorState
      title="Couldn't load pipeline details"
      error={lookup.error}
      onRetry={() => void lookup.refetch()}
      retrying={lookup.isFetching}
    />
  )
  const lookupPending = lookup.isPending && lookup.fetchStatus !== 'idle' && muted.length + watching.length > 0
  const kindsNote = hostNote ? `${KINDS_NOTE} ${hostNote}` : KINDS_NOTE

  const pipelineRows = (rows: PipelinePreference[]): ReactNode[] =>
    rows.map((row) => (
      <PipelinePreferenceRow
        key={row.key}
        name={row.name}
        detail={row.detail}
        action={row.action}
        label={row.label}
        mobileShell={mobileShell}
        onAction={row.run}
      />
    ))
  const kindSwitches = PREFERENCE_KINDS.map((kind) => (
    <KindSwitch key={kind} kind={kind} enabled={preferences[kind]} mobileShell={mobileShell} />
  ))
  const providerSwitches = providers.map((provider) => (
    <ProviderSwitch
      key={provider.id}
      provider={provider}
      enabled={!preferences.muted_providers.includes(provider.id)}
      mobileShell={mobileShell}
    />
  ))

  if (mobileShell) {
    return (
      <>
        <IosGroup header="Notify me about" footer={kindsNote}>
          {kindSwitches}
        </IosGroup>

        {providersState}
        {providers.length > 0 && (
          <IosGroup header="Providers" footer={PROVIDERS_NOTE}>
            {providerSwitches}
          </IosGroup>
        )}

        {lookupFailure}
        {lookupPending ? (
          <LoadingState message="Loading watched pipelines…" />
        ) : (
          <>
            <IosGroup header="Watched pipelines" footer={watching.length === 0 ? WATCHED_EMPTY : undefined}>
              {watching.length === 0 ? <NoneRow /> : pipelineRows(watching)}
            </IosGroup>

            <IosGroup header="Muted pipelines" footer={muted.length === 0 ? MUTED_EMPTY : undefined}>
              {muted.length === 0 ? <NoneRow /> : pipelineRows(muted)}
            </IosGroup>
          </>
        )}
      </>
    )
  }

  const watchedNote = watching.length === 0 ? WATCHED_EMPTY : 'These pipelines notify you when a run finishes.'
  const mutedNote = muted.length === 0 ? MUTED_EMPTY : 'These pipelines never notify you.'

  return (
    <>
      <Card padding="md">
        <Stack gap="md">
          <SectionHeading title="Notify me about" description={kindsNote} />
          {kindSwitches}
        </Stack>
      </Card>

      {providersState}
      {providers.length > 0 && (
        <Card padding="md">
          <Stack gap="md">
            <SectionHeading title="Providers" description={PROVIDERS_NOTE} />
            {providerSwitches}
          </Stack>
        </Card>
      )}

      {lookupFailure}
      {lookupPending ? (
        <LoadingState message="Loading watched pipelines…" />
      ) : (
        <>
          <Card padding="md">
            <Stack gap="md">
              <SectionHeading title="Watched pipelines" description={watchedNote} />
              {pipelineRows(watching)}
            </Stack>
          </Card>

          <Card padding="md">
            <Stack gap="md">
              <SectionHeading title="Muted pipelines" description={mutedNote} />
              {pipelineRows(muted)}
            </Stack>
          </Card>
        </>
      )}
    </>
  )
}
