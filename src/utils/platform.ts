import { isTauri } from '../services'

type Platform = 'macos' | 'windows' | 'linux';

let cachedPlatform: Platform | null = null

export function setPlatformOverride(platform: Platform | null): void {
  if (platform) {
    localStorage.setItem('platform-override', platform)
  } else {
    localStorage.removeItem('platform-override')
  }
  cachedPlatform = null
}

export function getPlatformOverride(): Platform | null {
  const override = localStorage.getItem('platform-override')


  
return override as Platform | null
}

function modernPlatformDetection(): Platform {
  if ('userAgentData' in navigator) {
    const userAgentData = (navigator as any).userAgentData


    if (userAgentData?.platform) {
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
  const override = getPlatformOverride()


  if (override) {
    return override
  }

  if (cachedPlatform) {
    return cachedPlatform
  }

  if (!isTauri()) {
    cachedPlatform = modernPlatformDetection()
    
return cachedPlatform
  }

  const { platform: getPlatform } = await import('@tauri-apps/plugin-os')
  const p = await getPlatform()

  cachedPlatform = p as Platform
  
return cachedPlatform
}
