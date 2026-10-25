import { modals } from '@mantine/modals'

import { isMobileShell } from '../hooks/useIsMobile'
import { MODAL_TRANSITION } from '../lib/motion'
import { iosClasses } from '../theme/ios'

import { watchActions } from './alertFit'

type ConfirmOptions = Parameters<typeof modals.openConfirmModal>[0]

const ALERT_Z_INDEX = 400

const fitActions = (anchor: HTMLElement | null) => {
  const group = anchor?.closest(`.${iosClasses.alertBody}`)?.querySelector<HTMLElement>(`.${iosClasses.alertActions}`)

  return group ? watchActions(group) : undefined
}

export function openConfirm(options: ConfirmOptions): string {
  if (!isMobileShell()) {
    return modals.openConfirmModal({
      zIndex: ALERT_Z_INDEX,
      centered: true,
      transitionProps: MODAL_TRANSITION,
      ...options,
      closeButtonProps: { ...options.closeButtonProps, 'aria-label': 'Close dialog' },
    })
  }

  const { cancelProps, children, confirmProps, groupProps, labels } = options
  const destructive = confirmProps?.color === 'red'
  const buttonClass = (preferred: boolean, extra = '') =>
    [iosClasses.alertButton, preferred ? iosClasses.alertButtonPreferred : '', extra].filter(Boolean).join(' ')

  return modals.openConfirmModal({
    zIndex: ALERT_Z_INDEX,
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
    children: <div ref={fitActions}>{children}</div>,
    groupProps: { ...groupProps, gap: 0, className: iosClasses.alertActions },
    cancelProps: { ...cancelProps, variant: 'transparent', color: undefined, className: buttonClass(destructive) },
    confirmProps: {
      ...confirmProps,
      variant: 'transparent',
      color: undefined,
      className: buttonClass(!destructive, destructive ? iosClasses.alertButtonDestructive : ''),
    },
    labels,
  })
}

let pendingDiscard: Promise<boolean> | null = null

export function confirmDiscard(): Promise<boolean> {
  if (pendingDiscard) {
    return pendingDiscard
  }

  const { promise, resolve } = Promise.withResolvers<boolean>()

  pendingDiscard = promise.finally(() => {
    pendingDiscard = null
  })

  openConfirm({
    title: 'Discard changes?',
    children: 'Your changes will be lost.',
    labels: isMobileShell()
      ? { confirm: 'Discard Changes', cancel: 'Keep Editing' }
      : { confirm: 'Discard changes', cancel: 'Keep editing' },
    confirmProps: { color: 'red' },
    onConfirm: () => resolve(true),
    onCancel: () => resolve(false),
    onClose: () => resolve(false),
  })

  return pendingDiscard
}
