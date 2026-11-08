'use client'
import { type ReactNode, useEffect, useRef } from 'react'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { detectPlatform } from '@/lib/defaults'
import {
  DEVICE_LABEL,
  deviceForOrientation,
  ORIENTATION_PAIRS,
  orientationOf,
  PLATFORM_DEVICES,
  supportsLandscape,
} from '@/lib/device-specs'
import type { Device, Orientation, Platform } from '@/lib/types'

function isPlatform(value: string): value is Platform {
  return value === 'ios' || value === 'macos' || value === 'android'
}

type Props = {
  device: Device
  setDevice: (v: Device) => void
  orientation: Orientation
  setOrientation: (v: Orientation) => void
  busy: boolean
}

export function ToolbarDevicePicker({
  device,
  setDevice,
  orientation: orientationProp,
  setOrientation,
  busy,
}: Props): ReactNode {
  const platform = detectPlatform(device)
  const hasLandscape = supportsLandscape(device)

  // Track last device per platform so the platform tabs preserve the user's choice.
  const lastByPlatform = useRef<Record<Platform, Device>>({
    ios: platform === 'ios' ? device : 'iphone',
    macos: platform === 'macos' ? device : 'mac',
    android: platform === 'android' ? device : 'android',
  })
  useEffect(() => {
    lastByPlatform.current[platform] = device
  }, [platform, device])

  // Paired orientations share one picker entry; the Orientation menu switches.
  const pickerDevice = deviceForOrientation(device, 'portrait')
  const platformDevices = PLATFORM_DEVICES[platform].filter((d) => deviceForOrientation(d, 'portrait') === d)
  const orientation = orientationOf(device, orientationProp)

  const changePlatform = (value: string) => {
    if (!busy && isPlatform(value)) {
      setDevice(lastByPlatform.current[value])
    }
  }
  const changeDevice = (value: string) => {
    const next = platformDevices.find((d) => d === value)
    if (!next) {
      return
    }
    // Moving between Duo displays keeps the orientation you're working in.
    const keep = ORIENTATION_PAIRS[device] ? orientation : 'portrait'
    setDevice(deviceForOrientation(next, keep))
  }
  const deviceOptions = platformDevices.map((d) => (
    <SelectItem key={d} value={d}>
      {DEVICE_LABEL[d]}
    </SelectItem>
  ))

  return (
    <>
      <Tabs value={platform} onValueChange={changePlatform}>
        <TabsList className="h-8 p-0.5">
          <TabsTrigger value="ios" className="h-7 px-3 text-xs" disabled={busy}>
            iOS
          </TabsTrigger>
          <TabsTrigger value="macos" className="h-7 px-3 text-xs" disabled={busy}>
            Mac
          </TabsTrigger>
          <TabsTrigger value="android" className="h-7 px-3 text-xs" disabled={busy}>
            Android
          </TabsTrigger>
        </TabsList>
      </Tabs>

      {/* Mac has a single device, so the tab alone says which deck is open. */}
      {platformDevices.length > 1 && (
        <Select value={pickerDevice} onValueChange={changeDevice} disabled={busy}>
          <SelectTrigger className="h-8 w-36 text-xs" aria-label="Device" title="Device">
            <SelectValue placeholder="Device">{DEVICE_LABEL[pickerDevice]}</SelectValue>
          </SelectTrigger>
          <SelectContent>{deviceOptions}</SelectContent>
        </Select>
      )}

      {hasLandscape && (
        <Select
          value={orientation}
          onValueChange={(v) => setOrientation(v === 'landscape' ? 'landscape' : 'portrait')}
          disabled={busy}
        >
          <SelectTrigger className="h-8 w-28 text-xs" aria-label="Orientation">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="portrait">Portrait</SelectItem>
            <SelectItem value="landscape">Landscape</SelectItem>
          </SelectContent>
        </Select>
      )}
    </>
  )
}
