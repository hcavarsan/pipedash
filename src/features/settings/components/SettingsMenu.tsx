import { Stack } from '@mantine/core'
import { IconLock } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { SETTINGS_SECTIONS, type SettingsSection } from '../../../components/common/SettingsSidebar'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useLockVault, useVaultStatus } from '../../../queries/useVaultQueries'
import { lockMeansSomething } from '../../../utils/appLock'

const GROUPS: SettingsSection[][] = [
  ['general', 'notifications', 'providers'],
  ['metrics', 'cache', 'storage'],
  ['about'],
]

export function SettingsMenu(): ReactNode {
  const { data: vaultStatus } = useVaultStatus()
  const lockVault = useLockVault()
  const canLock = vaultStatus ? lockMeansSomething(vaultStatus) && vaultStatus.is_unlocked : false

  return (
    <Stack gap="lg" pb="md">
      {GROUPS.map((ids) => (
        <IosGroup key={ids.join()}>
          {ids.map((id) => {
            const section = SETTINGS_SECTIONS.find((item) => item.id === id)

            if (!section) {
              return null
            }

            return <IosRow key={id} title={section.label} icon={section.icon} to={`/settings/${id}`} />
          })}
        </IosGroup>
      ))}
      {canLock && (
        <IosGroup footer="Pipedash asks you to unlock it again before it shows your pipelines.">
          <IosRow
            title="Lock Pipedash"
            icon={<IconLock size={18} />}
            disabled={lockVault.isPending}
            onClick={() => lockVault.mutate()}
          />
        </IosGroup>
      )}
    </Stack>
  )
}
