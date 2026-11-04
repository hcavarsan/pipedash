import { Card, Stack, Switch, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { ErrorState } from '@/components/common/ErrorState'
import { LoadingState } from '@/components/common/LoadingState'
import { IosGroup } from '@/components/ios/IosList'
import { SwitchRow } from '@/components/ios/mobileParts'
import { usePushRelay, useSetPushRelay } from '@/features/notifications/queries/usePushQueries'
import { useMobileShell } from '@/hooks/useIsMobile'

const TITLE = 'Push alerts for iPhone and iPad'
const LABEL = 'Send push alerts through relay.pipedash.app'
const DESCRIPTION =
  "Apple delivers push alerts only through a relay that holds Pipedash's signing key. With this on, this server sends the relay only an alert ID, and each iPhone or iPad downloads the alert from this server. The relay never sees pipeline names, but it sees when alerts happen."

export function PushRelayCard(): ReactNode {
  const { data, error, isLoading, isRefetching, refetch } = usePushRelay()
  const setRelay = useSetPushRelay()
  const mobileShell = useMobileShell()

  if (error) {
    return (
      <ErrorState
        title="Couldn't load push alert settings"
        error={error}
        onRetry={() => refetch()}
        retrying={isRefetching}
      />
    )
  }

  if (isLoading || !data) {
    return <LoadingState message="Loading push alert settings" />
  }

  let note: string | undefined
  if (data.from_env) {
    note = 'Set by PIPEDASH_PUSH_RELAY_URL.'
  } else if (!data.enabled) {
    note = 'Phones still check this server in the background, with delays.'
  }
  const disabled = data.from_env || setRelay.isPending

  if (mobileShell) {
    return (
      <IosGroup header={TITLE} footer={note ? `${DESCRIPTION} ${note}` : DESCRIPTION}>
        <SwitchRow
          title={LABEL}
          checked={data.enabled}
          disabled={disabled}
          onChange={(event) => setRelay.mutate(event.currentTarget.checked)}
        />
      </IosGroup>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <Stack gap={0}>
          <Text size="sm" fw={600}>
            {TITLE}
          </Text>
          <Text size="xs" c="dimmed">
            {DESCRIPTION}
          </Text>
        </Stack>

        <Switch
          label={LABEL}
          description={note}
          checked={data.enabled}
          disabled={disabled}
          onChange={(event) => setRelay.mutate(event.currentTarget.checked)}
        />
      </Stack>
    </Card>
  )
}
