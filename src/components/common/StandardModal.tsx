import { Box, Drawer, Loader, Modal, ScrollArea, UnstyledButton } from '@mantine/core'
import { useReducedMotion } from '@mantine/hooks'
import { IconChevronLeft } from '@tabler/icons-react'
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { registerSheet, type SheetHandle, useSheetStack } from '../../hooks/useSheetStack'
import { PRESENT_EASING } from '../../lib/motion'
import type { SheetDetent } from '../../lib/sheetDetents'
import { iosClasses } from '../../theme/ios'
import { TYPOGRAPHY } from '../../theme/typography'
import type { ModalBaseProps } from '../../types'
import { confirmDiscard } from '../../utils/confirm'

type ModalSize = 'md' | 'lg' | 'xl' | 'full'

export interface ModalAction {
  label: string
  onClick: () => void
  loading?: boolean | undefined
  disabled?: boolean | undefined
  destructive?: boolean | undefined
}

interface StandardModalProps extends ModalBaseProps {
  title: ReactNode
  children: ReactNode
  footer?: ReactNode | ((requestClose: () => void) => ReactNode)
  loading?: boolean
  closable?: boolean
  size?: ModalSize
  contentPadding?: boolean
  disableScrollArea?: boolean
  fullHeight?: boolean
  primaryAction?: ModalAction
  backAction?: { label: string; onClick: () => void }
  dirty?: boolean
  detents?: SheetDetent[]
}

const DESKTOP_LAYOUT: Record<ModalSize, { width: string; height?: string; xOffset?: string }> = {
  md: { width: 'min(560px, 92vw)' },
  lg: { width: 'min(880px, 92vw)' },
  xl: { width: 'min(1400px, 95vw)', height: 'min(90vh, 1000px)' },
  full: { width: '96vw', height: '94vh', xOffset: '2vw' },
}

const CLOSE_WAIT_MS = 600

const columnStyle = { display: 'flex', flexDirection: 'column', minHeight: 0, overflow: 'hidden' } as const

interface HeaderButtonProps {
  children: ReactNode
  onClick: () => void
  strong?: boolean | undefined
  destructive?: boolean | undefined
  disabled?: boolean | undefined
  loading?: boolean | undefined
}

const HeaderButton = ({ children, onClick, strong, destructive, disabled, loading }: HeaderButtonProps) => (
  <UnstyledButton
    type="button"
    className={iosClasses.sheetAction}
    data-strong={strong || undefined}
    data-destructive={destructive || undefined}
    data-loading={loading || undefined}
    disabled={disabled || loading}
    aria-busy={loading || undefined}
    onClick={onClick}
  >
    <span className={iosClasses.sheetActionLabel}>{children}</span>
    {loading && <Loader size={16} className={iosClasses.sheetActionSpinner} aria-hidden />}
  </UnstyledButton>
)

export const StandardModal = ({
  opened,
  onClose,
  title,
  children,
  footer,
  loading = false,
  closable = true,
  size = 'lg',
  contentPadding = true,
  disableScrollArea = false,
  fullHeight = size === 'xl' || size === 'full',
  primaryAction,
  backAction,
  dirty = false,
  detents,
}: StandardModalProps): ReactNode => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const reducedMotion = useReducedMotion()
  const { id, zIndex, covered } = useSheetStack(opened && mobileShell)
  const [instant, setInstant] = useState(false)
  const exited = useRef<(() => void) | null>(null)
  const latest = useRef({ dirty, onClose })

  latest.current = { dirty, onClose }

  const busy = loading || Boolean(primaryAction?.loading)
  const locked = busy || !closable
  const padding = contentPadding ? (mobileShell ? '16px' : 'var(--mantine-spacing-lg)') : '0px'
  const hasFooter = Boolean(footer) && !(mobileShell && primaryAction)
  const bottomInset = mobileShell && !hasFooter ? `calc(${padding} + var(--sheet-pb, 0px))` : undefined

  const requestClose = useCallback(async () => {
    if (latest.current.dirty && !(await confirmDiscard())) {
      return false
    }

    latest.current.onClose()

    return true
  }, [])

  const closeNow = useCallback(() => {
    const { promise, resolve } = Promise.withResolvers<boolean>()

    exited.current = () => resolve(true)
    setInstant(true)
    latest.current.onClose()
    window.setTimeout(() => {
      if (exited.current) {
        exited.current = null
        setInstant(false)
        resolve(false)
      }
    }, CLOSE_WAIT_MS)

    return promise
  }, [])

  const handle = useMemo<SheetHandle>(() => ({ requestClose, closeNow }), [requestClose, closeNow])

  useEffect(() => registerSheet(id, handle), [id, handle])

  const onExitTransitionEnd = useCallback(() => {
    exited.current?.()
    exited.current = null
    setInstant(false)
  }, [])

  const sharedProps = {
    opened,
    onClose: requestClose,
    title,
    closeOnClickOutside: !locked,
    closeOnEscape: !locked,
    withCloseButton: !locked,
    closeButtonProps: { 'aria-label': 'Close dialog' },
    zIndex: 300,
  }

  const sharedStyles = {
    title: {
      fontWeight: TYPOGRAPHY.MODAL_TITLE.weight,
      fontSize: `var(--mantine-font-size-${TYPOGRAPHY.MODAL_TITLE.size})`,
    },
    header: {
      borderBottom: '1px solid var(--mantine-color-default-border)',
      paddingInline: 'var(--mantine-spacing-lg)',
    },
    body: { ...columnStyle, flex: 1, padding: 0 },
  }

  const content = (
    <>
      {disableScrollArea ? (
        <Box style={{ ...columnStyle, flex: 1, padding, paddingBottom: bottomInset }}>{children}</Box>
      ) : (
        <ScrollArea
          type={mobileShell ? 'scroll' : 'auto'}
          {...(mobileShell ? { scrollbarSize: 3, scrollHideDelay: 200 } : {})}
          style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}
          styles={{ viewport: { flex: 1, minHeight: 0, height: 'auto' } }}
        >
          <Box p={padding} {...(bottomInset === undefined ? {} : { pb: bottomInset })}>
            {children}
          </Box>
        </ScrollArea>
      )}
      {hasFooter && (
        <Box
          className={mobileShell ? iosClasses.sheetFooter : undefined}
          px="lg"
          py="md"
          style={{
            flexShrink: 0,
            borderTop: '1px solid var(--mantine-color-default-border)',
            paddingBottom: mobileShell
              ? 'calc(var(--mantine-spacing-md) + var(--sheet-pb, 0px))'
              : isMobile
                ? 'calc(var(--mantine-spacing-md) + var(--safe-bottom))'
                : undefined,
          }}
        >
          {typeof footer === 'function' ? footer(requestClose) : footer}
        </Box>
      )}
    </>
  )

  if (mobileShell) {
    const detentList = detents && detents.length > 0 ? detents : undefined
    const startDetent = detentList
      ? detentList.includes('medium')
        ? 'medium'
        : 'large'
      : fullHeight
        ? 'large'
        : 'auto'
    const showCancel = closable && !backAction && Boolean(primaryAction || footer)
    const showDone = closable && !primaryAction && !footer

    return (
      <Modal.Root
        opened={opened}
        onClose={requestClose}
        zIndex={zIndex}
        radius={0}
        closeOnClickOutside={!locked}
        closeOnEscape={!locked && covered === 0}
        trapFocus={covered === 0}
        transitionProps={{
          transition: reducedMotion ? 'fade' : 'slide-up',
          duration: instant ? 0 : reducedMotion ? 200 : 320,
          timingFunction: PRESENT_EASING,
        }}
        onExitTransitionEnd={onExitTransitionEnd}
        attributes={{
          inner: { 'data-sheet-covered': covered > 0 ? covered : undefined },
          content: {
            'data-sheet-id': id,
            'data-detent': startDetent,
            'data-detents': detentList?.join(','),
            'data-dirty': dirty ? '' : undefined,
            'data-locked': locked ? '' : undefined,
            inert: covered > 0 ? true : undefined,
          },
        }}
        styles={{
          body: { ...sharedStyles.body, paddingTop: contentPadding ? 0 : 16 },
          content: columnStyle,
        }}
      >
        <Modal.Overlay color="#000" backgroundOpacity={0.45} />
        <Modal.Content>
          <Modal.Header>
            <div className={iosClasses.sheetLeading}>
              {backAction ? (
                <HeaderButton onClick={backAction.onClick} disabled={busy}>
                  <IconChevronLeft size={22} aria-hidden className={iosClasses.sheetBackIcon} />
                  {backAction.label}
                </HeaderButton>
              ) : (
                showCancel && (
                  <HeaderButton onClick={requestClose} disabled={busy}>
                    Cancel
                  </HeaderButton>
                )
              )}
            </div>
            <Modal.Title>{title}</Modal.Title>
            <div className={iosClasses.sheetTrailing}>
              {primaryAction ? (
                <HeaderButton
                  strong
                  destructive={primaryAction.destructive}
                  disabled={primaryAction.disabled || loading}
                  loading={primaryAction.loading}
                  onClick={primaryAction.onClick}
                >
                  {primaryAction.label}
                </HeaderButton>
              ) : (
                showDone && (
                  <HeaderButton strong onClick={requestClose} disabled={busy}>
                    Done
                  </HeaderButton>
                )
              )}
            </div>
          </Modal.Header>
          <Modal.Body>{content}</Modal.Body>
        </Modal.Content>
      </Modal.Root>
    )
  }

  if (isMobile) {
    return (
      <Drawer
        {...sharedProps}
        position="bottom"
        size="100%"
        styles={{
          ...sharedStyles,
          header: {
            ...sharedStyles.header,
            paddingTop: 'calc(var(--mantine-spacing-md) + var(--safe-top))',
          },
          content: columnStyle,
        }}
      >
        {content}
      </Drawer>
    )
  }

  const layout = DESKTOP_LAYOUT[size]

  return (
    <Modal
      {...sharedProps}
      centered
      size={layout.width}
      xOffset={layout.xOffset}
      styles={{
        ...sharedStyles,
        header: { ...sharedStyles.header, flexShrink: 0 },
        content: { ...columnStyle, height: layout.height, maxHeight: layout.height ?? 'min(85vh, 960px)' },
      }}
    >
      {content}
    </Modal>
  )
}
