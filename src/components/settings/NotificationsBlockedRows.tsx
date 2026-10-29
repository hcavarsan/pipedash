import type { ReactNode } from 'react'

import { mobileService } from '../../services/mobile'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { isMobilePlatform } from '../../utils/platform'
import { notificationProblem } from '../../utils/systemNotifications'
import { IosRow } from '../ios/IosList'

import { AccentText } from './mobileParts'

const WEB_HELP =
  "Allow notifications for this site in your browser's site settings, then turn this on again. On iPhone, add Pipedash to the Home Screen first, then open Settings > Notifications > Pipedash."

export function NotificationsBlockedRows(): ReactNode {
  const native = isMobilePlatform()

  return (
    <>
      <IosRow title="Notifications blocked" subtitle={native ? notificationProblem('blocked') : WEB_HELP} />
      {native && (
        <IosRow
          title={<AccentText>Open Settings</AccentText>}
          chevron
          onClick={() => {
            mobileService
              .openAppSettings()
              .catch((error: unknown) => displayErrorNotification(error, "Couldn't open Settings"))
          }}
        />
      )}
    </>
  )
}
