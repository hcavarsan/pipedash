import { platform as getPlatform } from '@tauri-apps/plugin-os'

type Platform = 'macos' | 'windows' | 'linux';

let cachedPlatform: Platform | null = null

/**
 * Get the current platform. Result is cached after first call.
 */
export async function platform(): Promise<Platform> {
  if (cachedPlatform) {
    return cachedPlatform
  }

  const p = await getPlatform()


  cachedPlatform = p as Platform
  
return cachedPlatform
}

