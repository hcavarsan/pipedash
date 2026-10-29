import { displayErrorNotification } from '../../utils/errorDisplay'
import { openExternal } from '../../utils/openExternal'

export const openNotificationLink = (url: string): void => {
  openExternal(url).catch((error: unknown) => displayErrorNotification(error, "Couldn't open link"))
}
