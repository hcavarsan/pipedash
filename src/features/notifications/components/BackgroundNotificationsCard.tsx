import { Card, Switch } from '@mantine/core'
import type { ReactNode } from 'react'

import { QueryBoundary } from '@/components/common/QueryBoundary'
import { IosGroup } from '@/components/ios/IosList'
import { SwitchRow } from '@/components/ios/mobileParts'
import { usePushRelay, useSetPushRelay } from '@/features/notifications/queries/usePushQueries'
import { useMobileShell } from '@/hooks/useIsMobile'
import { isTauri } from '@/services/isTauri'
import type { PushRelaySettings } from '@/types/notifications'

const LABEL = 'Background notifications'
const SENDER = isTauri() ? 'While this computer shares Pipedash, it' : 'This server'
const DESCRIPTION = `Send alerts to paired iPhones and iPads even when Pipedash is closed on them. ${SENDER} sends relay.pipedash.app only an alert ID, and each device downloads the alert itself. The relay never sees pipeline names, but it sees when alerts happen.`

function RelayControls({ data }: { data: PushRelaySettings }): ReactNode {
  const setRelay = useSetPushRelay()
  const mobileShell = useMobileShell()
  let note: string | undefined
  if (data.from_env) {
    note = 'Set by PIPEDASH_PUSH_RELAY_URL.'
  } else if (!data.enabled) {
    note = 'Phones still check for alerts in the background, with delays.'
  }
  const disabled = data.from_env || setRelay.isPending
  const description = note ? `${DESCRIPTION} ${note}` : DESCRIPTION

  if (mobileShell) {
    return (
      <IosGroup footer={description}>
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
      <Switch
        label={LABEL}
        aria-label={LABEL}
        description={description}
        checked={data.enabled}
        disabled={disabled}
        onChange={(event) => setRelay.mutate(event.currentTarget.checked)}
      />
    </Card>
  )
}

export function BackgroundNotificationsCard(): ReactNode {
  return (
    <QueryBoundary
      query={usePushRelay()}
      loading="Loading background notification settings…"
      failure="Couldn't load background notification settings"
    >
      {(data) => <RelayControls data={data} />}
    </QueryBoundary>
  )
}
