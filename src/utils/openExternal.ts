import { service } from '../services'

const HTTP_URL = /^https?:\/\//i

export const isHttpUrl = (url: string): boolean => HTTP_URL.test(url)

export async function openExternal(url: string): Promise<boolean> {
  if (!isHttpUrl(url)) {
    return false
  }

  await service.openUrl(url)

  return true
}
