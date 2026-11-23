import { Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { IosRow } from '../../../components/ios/IosList'
import { useNumberDraft } from '../../../hooks/useNumberDraft'
import { useStorageConfig } from '../../../queries/useStorageQueries'
import { RetentionDaysInput } from '../../metrics/components/RetentionDaysInput'
import { useUpdateGeneralSettings } from '../../settings/queries/useGeneralSettingsQueries'

const TITLE = 'Queue limit (minutes)'
const DESCRIPTION = 'A queued run past this limit is marked as stuck.'

export function QueueLimitInput({ mobileShell }: { mobileShell: boolean }): ReactNode {
  const storageConfig = useStorageConfig()
  const general = storageConfig.data?.config.general
  const mutation = useUpdateGeneralSettings({ toastErrors: false })
  const draft = useNumberDraft({
    saved: general?.queue_alert_minutes ?? 15,
    min: 1,
    save: async (minutes) => {
      if (general) {
        await mutation.mutateAsync({ ...general, queue_alert_minutes: minutes })
      }
    },
  })
  const { error } = draft

  if (storageConfig.error) {
    return (
      <ErrorState
        title="Couldn't load the queue limit"
        error={storageConfig.error}
        onRetry={() => void storageConfig.refetch()}
        retrying={storageConfig.isRefetching}
      />
    )
  }

  const input = (
    <RetentionDaysInput
      row={mobileShell}
      max={1440}
      value={draft.value}
      onChange={draft.onChange}
      onBlur={draft.commit}
      onEnter={draft.commit}
      error={mobileShell ? Boolean(error) : (error ?? undefined)}
      disabled={!general || draft.pending}
      {...(mobileShell ? { 'aria-label': TITLE } : { label: TITLE, description: DESCRIPTION })}
    />
  )
  const subtitle = error ? (
    <Text span inherit c="red">
      {error}
    </Text>
  ) : (
    DESCRIPTION
  )

  return (
    <>
      <LiveStatus message={error ?? ''} />
      {mobileShell ? <IosRow title={TITLE} subtitle={subtitle} accessory={input} /> : input}
    </>
  )
}
