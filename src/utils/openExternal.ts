import { service } from '../services/service'
import { displayErrorNotification } from './errorDisplay'

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

export function openLink(url: string): void {
  openExternal(url)
    .then((opened) => {
      if (!opened) {
        displayErrorNotification(new Error('The link is not a valid address.'), "Couldn't open link")
      }
    })
    .catch((error: unknown) => displayErrorNotification(error, "Couldn't open link"))
}
