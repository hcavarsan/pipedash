import { modals } from '@mantine/modals'

import { isMobileShell } from '../hooks/useIsMobile'
import { iosClasses } from '../theme/ios'

type ConfirmOptions = Parameters<typeof modals.openConfirmModal>[0]

const SHORT_LABEL = 14

export function openConfirm(options: ConfirmOptions): string {
  if (!isMobileShell()) {
    return modals.openConfirmModal(options)
  }

  const { cancelProps, confirmProps, groupProps, labels } = options
  const destructive = confirmProps?.color === 'red'
  const sideBySide = [labels?.cancel, labels?.confirm].every(
    (label) => typeof label !== 'string' || label.length <= SHORT_LABEL
  )

  return modals.openConfirmModal({
    ...options,
    centered: true,
    withCloseButton: false,
    transitionProps: { transition: 'pop', duration: 180 },
    classNames: {
      inner: iosClasses.alertInner,
      content: iosClasses.alertContent,
      header: iosClasses.alertHeader,
      title: iosClasses.alertTitle,
      body: iosClasses.alertBody,
    },
    groupProps: {
      ...groupProps,
      gap: 0,
      mod: { layout: sideBySide ? 'row' : 'column' },
      className: iosClasses.alertActions,
    },
    cancelProps: { ...cancelProps, variant: 'transparent', color: undefined, className: iosClasses.alertButton },
    confirmProps: {
      ...confirmProps,
      variant: 'transparent',
      color: undefined,
      className: `${iosClasses.alertButton} ${iosClasses.alertButtonPrimary} ${destructive ? iosClasses.alertButtonDestructive : ''}`,
    },
  })
}
