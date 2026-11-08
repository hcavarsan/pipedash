import { Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { IosGroup } from '../../../components/ios/IosList'

interface TriggerWorkflowBodyProps {
  fetchError: Error | null
  retrying: boolean
  onRetry: () => void
  ready: boolean
  blockedReason: string | undefined
  note: string | null | undefined
  parameterCount: number
  mobileShell: boolean
  isMobile: boolean
  fields: ReactNode[]
}

export function TriggerWorkflowBody({
  fetchError,
  retrying,
  onRetry,
  ready,
  blockedReason,
  note,
  parameterCount,
  mobileShell,
  isMobile,
  fields,
}: TriggerWorkflowBodyProps): ReactNode {
  if (fetchError) {
    return (
      <ErrorState
        title="Couldn't load workflow parameters"
        error={fetchError}
        hint="The workflow can't be triggered until its parameters load."
        onRetry={onRetry}
        retrying={retrying}
      />
    )
  }

  if (!ready) {
    return <LoadingState message="Loading workflow parameters..." />
  }

  if (blockedReason || (note && parameterCount === 0)) {
    return (
      <Text size="sm" c={blockedReason ? 'orange' : 'dimmed'}>
        {blockedReason ?? note}
      </Text>
    )
  }

  if (parameterCount === 0) {
    return (
      <Text size="sm" c="dimmed">
        This workflow has no parameters. It runs with its defaults.
      </Text>
    )
  }

  if (mobileShell) {
    return <IosGroup>{fields}</IosGroup>
  }

  return <Stack gap={isMobile ? 'xs' : 'sm'}>{fields}</Stack>
}
