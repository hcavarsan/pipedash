import { type ReactNode, useState } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { IosRow } from '../../../components/ios/IosList'
import { useStorageConfig } from '../../../queries/useStorageQueries'
import { RetentionDaysInput } from '../../metrics/components/RetentionDaysInput'
import { useUpdateGeneralSettings } from '../../settings/queries/useGeneralSettingsQueries'

const TITLE = 'Queue limit (minutes)'
const DESCRIPTION = 'A queued run past this limit is marked as stuck.'

export function QueueLimitInput({ mobileShell }: { mobileShell: boolean }): ReactNode {
  const storageConfig = useStorageConfig()
  const general = storageConfig.data?.config.general
  const mutation = useUpdateGeneralSettings()
  const [draft, setDraft] = useState<string | number | null>(null)
  const saved = general?.queue_alert_minutes ?? 15
  const value = draft ?? saved

  const commit = (): void => {
    setDraft(null)
    if (general && typeof value === 'number' && value !== saved) {
      mutation.mutate({ ...general, queue_alert_minutes: value })
    }
  }
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
      value={value}
      onChange={setDraft}
      onBlur={commit}
      onEnter={commit}
      disabled={!general || mutation.isPending}
      {...(mobileShell ? { 'aria-label': TITLE } : { label: TITLE, description: DESCRIPTION })}
    />
  )

  return mobileShell ? <IosRow title={TITLE} subtitle={DESCRIPTION} accessory={input} /> : input
}
