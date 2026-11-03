import { Stack } from '@mantine/core'
import type { ReactNode } from 'react'

import { useMobileShell } from '@/hooks/useIsMobile'
import { hasLocalCore, isRemoteClient } from '@/services/connection'
import { isTauri } from '@/services/isTauri'
import { isDesktopTauri } from '@/utils/platform'

import { PairedDevicesCard } from './PairedDevicesCard'
import { PushRelayCard } from './PushRelayCard'
import { SharingCard } from './SharingCard'
import { ThisDeviceCard } from './ThisDeviceCard'

export function DevicesSection(): ReactNode {
  const mobileShell = useMobileShell()
  const sharing = isDesktopTauri() && hasLocalCore()

  return (
    <Stack gap={mobileShell ? 'lg' : 'md'} pb={mobileShell ? 'md' : 0}>
      {isTauri() && <ThisDeviceCard />}
      {sharing && <SharingCard />}
      {!isTauri() && <PushRelayCard />}
      {(!isTauri() || isRemoteClient() || sharing) && <PairedDevicesCard sharing={sharing} />}
    </Stack>
  )
}
