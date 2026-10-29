import { platform as tauriOsPlatform } from '@tauri-apps/plugin-os'

import { isTauri } from '../services/isTauri'

import { isOneOf } from './isOneOf'
import { isRecord } from './isRecord'

const PLATFORMS = ['macos', 'windows', 'linux', 'ios', 'android'] as const

export type Platform = (typeof PLATFORMS)[number]

let cachedPlatform: Platform | null = null
let cachedTauriPlatform: Platform | null = null

export function setPlatformOverride(platform: 'macos' | 'windows' | 'linux' | null): void {
  if (platform) {
    localStorage.setItem('platform-override', platform)
  } else {
    localStorage.removeItem('platform-override')
  }
  cachedPlatform = null
}

export function getPlatformOverride(): Platform | null {
  const override = localStorage.getItem('platform-override')

  return isOneOf(PLATFORMS, override) ? override : null
}

export function normalizePlatform(name: string): Platform {
  switch (name) {
    case 'macos':
    case 'windows':
    case 'ios':
    case 'android':
      return name
    default:
      return 'linux'
  }
}

export const isMobileOs = (p: Platform): boolean => p === 'ios' || p === 'android'

function tauriPlatform(): Platform {
  cachedTauriPlatform ??= normalizePlatform(tauriOsPlatform())

  return cachedTauriPlatform
}

export const isMobilePlatform = (): boolean => isTauri() && isMobileOs(tauriPlatform())

export const isAndroidPlatform = (): boolean => isTauri() && tauriPlatform() === 'android'

export const secureStoreLabel = (): string => (isAndroidPlatform() ? 'Android Keystore' : 'Device Keychain')

export const isDesktopTauri = (): boolean => isTauri() && !isMobilePlatform()

function modernPlatformDetection(): Platform {
  if ('userAgentData' in navigator && isRecord(navigator.userAgentData)) {
    const userAgentData = navigator.userAgentData

    if (typeof userAgentData.platform === 'string' && userAgentData.platform) {
      const platform = userAgentData.platform.toLowerCase()

      if (platform.includes('mac')) {
        return 'macos'
      }
      if (platform.includes('win')) {
        return 'windows'
      }
      if (platform.includes('linux')) {
        return 'linux'
      }
    }
  }

  return fallbackPlatformDetection()
}

function fallbackPlatformDetection(): Platform {
  const ua = navigator.userAgent.toLowerCase()

  if (ua.includes('mac')) {
    return 'macos'
  }
  if (ua.includes('win')) {
    return 'windows'
  }

  return 'linux'
}

export async function platform(): Promise<Platform> {
  if (isMobilePlatform()) {
    return tauriPlatform()
  }

  const override = getPlatformOverride()

  if (override) {
    return override
  }

  if (cachedPlatform) {
    return cachedPlatform
  }

  cachedPlatform = isTauri() ? tauriPlatform() : modernPlatformDetection()

  return cachedPlatform
}
