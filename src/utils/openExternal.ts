import { service } from '../services'

export const isHttpUrl = (url: string): boolean => {
  if (!URL.canParse(url)) {
    return false
  }

  const parsed = new URL(url)

  return (parsed.protocol === 'http:' || parsed.protocol === 'https:') && parsed.hostname !== ''
}

export async function openExternal(url: string): Promise<boolean> {
  if (!isHttpUrl(url)) {
    return false
  }

  await service.openUrl(url)

  return true
}
