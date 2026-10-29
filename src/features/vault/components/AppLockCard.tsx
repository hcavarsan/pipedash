import { Card, Stack, Switch, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { IosGroup } from '../../../components/ios/IosList'
import { SwitchRow } from '../../../components/ios/mobileParts'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useSetAppLock, useVaultStatus } from '../../../queries/useVaultQueries'
import { biometryLabel } from '../../../utils/appLock'

export function AppLockCard(): ReactNode {
  const mobileShell = useMobileShell()
  const { data: vaultStatus } = useVaultStatus()
  const setAppLock = useSetAppLock()
  const appLock = vaultStatus?.app_lock

  if (!appLock?.supported) {
    return null
  }

  const label = biometryLabel(appLock.biometry)
  const description = appLock.biometry_available
    ? `Ask for ${label} before Pipedash shows your pipelines and tokens.`
    : `${label.charAt(0).toUpperCase()}${label.slice(1)} isn't set up on this device. Pipedash asks for your device passcode instead.`

  if (mobileShell) {
    return (
      <IosGroup header="Security" footer={description}>
        <SwitchRow
          title={`Require ${label}`}
          checked={appLock.enabled}
          disabled={setAppLock.isPending}
          onChange={(e) =>
            setAppLock.mutate({
              enabled: e.currentTarget.checked,
              lockOnBackground: e.currentTarget.checked && appLock.lock_on_background,
            })
          }
        />
        <SwitchRow
          title="Lock when leaving the app"
          subtitle="Lock Pipedash each time you switch to another app."
          checked={appLock.enabled && appLock.lock_on_background}
          disabled={!appLock.enabled || setAppLock.isPending}
          onChange={(e) => setAppLock.mutate({ enabled: true, lockOnBackground: e.currentTarget.checked })}
        />
      </IosGroup>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <Text size="sm" fw={600}>
          Security
        </Text>

        <Switch
          label={`Require ${label}`}
          description={description}
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
