import { Loader, Modal, UnstyledButton } from '@mantine/core'
import { IconChevronLeft } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { iosClasses } from '../../theme/ios'

import type { HeaderButtonProps, ModalAction, StandardModalProps } from './standardModalTypes'

interface SheetHeaderProps {
  title: ReactNode
  backAction: StandardModalProps['backAction']
  primaryAction: ModalAction | undefined
  busy: boolean
  loading: boolean
  showCancel: boolean
  showDone: boolean
  onClose: () => void
}

const HeaderButton = ({
  children,
  ariaLabel,
  onClick,
  strong,
  destructive,
  disabled,
  loading,
}: HeaderButtonProps): ReactNode => (
  <UnstyledButton
    type="button"
    className={iosClasses.sheetAction}
    data-strong={strong || undefined}
    data-destructive={destructive || undefined}
    data-loading={loading || undefined}
    disabled={disabled || loading}
    aria-busy={loading || undefined}
    aria-label={ariaLabel}
    onClick={onClick}
  >
    <span className={iosClasses.sheetActionLabel}>{children}</span>
    {loading && <Loader size={16} className={iosClasses.sheetActionSpinner} aria-hidden />}
  </UnstyledButton>
)

export const SheetHeader = ({
  title,
  backAction,
  primaryAction,
  busy,
  loading,
  showCancel,
  showDone,
  onClose,
}: SheetHeaderProps): ReactNode => {
  let leading: ReactNode = showCancel && (
    <HeaderButton onClick={onClose} disabled={busy}>
      Cancel
    </HeaderButton>
  )

  if (backAction) {
    leading = (
      <HeaderButton
        onClick={backAction.onClick}
        disabled={busy}
        ariaLabel={backAction.minimal ? backAction.label : undefined}
      >
        <IconChevronLeft size={22} aria-hidden className={iosClasses.sheetBackIcon} />
        {!backAction.minimal && backAction.label}
      </HeaderButton>
    )
  }

  let trailing: ReactNode = showDone && (
    <HeaderButton strong onClick={onClose} disabled={busy}>
      Done
    </HeaderButton>
  )

  if (primaryAction) {
    trailing = (
      <HeaderButton
        strong
        destructive={primaryAction.destructive}
        disabled={primaryAction.disabled || loading}
        loading={primaryAction.loading}
        onClick={primaryAction.onClick}
      >
        {primaryAction.label}
      </HeaderButton>
    )
  }

  return (
    <Modal.Header>
      <div className={iosClasses.sheetLeading}>{leading}</div>
      <Modal.Title>{title}</Modal.Title>
      <div className={iosClasses.sheetTrailing}>{trailing}</div>
    </Modal.Header>
  )
}
