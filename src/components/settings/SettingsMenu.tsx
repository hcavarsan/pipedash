import { Stack } from '@mantine/core'
import type { ReactNode } from 'react'

import { IosGroup, IosRow } from '../ios/IosList'
import { SETTINGS_SECTIONS, type SettingsSection } from './SettingsSidebar'

const GROUPS: SettingsSection[][] = [
  ['general', 'notifications', 'providers'],
  ['metrics', 'cache', 'storage'],
  ['about'],
]

export function SettingsMenu(): ReactNode {
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
    </Stack>
  )
}
