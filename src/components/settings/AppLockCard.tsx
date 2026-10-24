import { Card, Stack, Switch, Text } from '@mantine/core'

import { useSetAppLock, useVaultStatus } from '../../queries/useVaultQueries'
import { biometryLabel } from '../../utils/appLock'

export function AppLockCard() {
  const { data: vaultStatus } = useVaultStatus()
  const setAppLock = useSetAppLock()
  const appLock = vaultStatus?.app_lock

  if (!appLock?.supported) {
    return null
  }

  const label = biometryLabel(appLock.biometry)

  return (
    <Card padding="md">
      <Stack gap="md">
        <Text size="sm" fw={600}>
          Security
        </Text>

        <Switch
          label={`Require ${label}`}
          description={
            appLock.biometry_available
              ? `Ask for ${label} before Pipedash shows your pipelines and tokens.`
              : `${label} isn't set up on this device. Pipedash asks for your device passcode instead.`
          }
          checked={appLock.enabled}
          disabled={setAppLock.isPending}
          onChange={(e) =>
            setAppLock.mutate({
              enabled: e.currentTarget.checked,
              lockOnBackground: e.currentTarget.checked && appLock.lock_on_background,
            })
          }
        />

        <Switch
          label="Lock when leaving the app"
          description="Lock Pipedash each time you switch to another app."
          checked={appLock.enabled && appLock.lock_on_background}
          disabled={!appLock.enabled || setAppLock.isPending}
          onChange={(e) => setAppLock.mutate({ enabled: true, lockOnBackground: e.currentTarget.checked })}
        />
      </Stack>
    </Card>
  )
}
