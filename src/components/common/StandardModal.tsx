import { Box, Drawer, Modal } from '@mantine/core'
import { useReducedMotion } from '@mantine/hooks'
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { registerSheet, type SheetHandle, useSheetStack } from '../../hooks/useSheetStack'
import { PRESENT_EASING } from '../../lib/motion'
import { iosClasses } from '../../theme/ios'
import { TYPOGRAPHY } from '../../theme/typography'
import { confirmDiscard } from '../../utils/confirm'
import { ErrorBoundary } from '../ErrorBoundary/ErrorBoundary'
import { FillScrollArea } from './FillScrollArea'
import { SheetHeader } from './SheetHeader'
import type { ModalSize, StandardModalProps } from './standardModalTypes'

const DESKTOP_LAYOUT: Record<ModalSize, { width: string; height?: string; xOffset?: string }> = {
  md: { width: 'min(560px, 92vw)' },
  lg: { width: 'min(880px, 92vw)' },
  xl: { width: 'min(1400px, 95vw)', height: 'min(90vh, 1000px)' },
  full: { width: '96vw', height: '94vh', xOffset: '2vw' },
}

const CLOSE_WAIT_MS = 600

const columnStyle = { display: 'flex', flexDirection: 'column', minHeight: 0, overflow: 'hidden' } as const

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
  const exited = useRef<((closed: boolean) => void) | null>(null)
  const closeTimer = useRef<number | null>(null)
  const latest = useRef({ dirty, onClose })

  latest.current = { dirty, onClose }

  const busy = loading || Boolean(primaryAction?.loading)
  const locked = busy || !closable
  let padding = '0px'

  if (contentPadding) {
    padding = mobileShell ? '16px' : 'var(--mantine-spacing-lg)'
  }

  const hasFooter = Boolean(footer) && !(mobileShell && primaryAction)
  const bottomInset = mobileShell && !hasFooter ? `calc(${padding} + var(--sheet-pb, 0px))` : undefined

  const requestClose = useCallback(async () => {
    if (latest.current.dirty && !(await confirmDiscard())) {
      return false
    }

    latest.current.onClose()

    return true
  }, [])

  const settlePending = useCallback((closed: boolean) => {
    if (closeTimer.current !== null) {
      window.clearTimeout(closeTimer.current)
      closeTimer.current = null
    }

    const resolve = exited.current

    exited.current = null
    resolve?.(closed)
  }, [])

  const closeNow = useCallback(() => {
    const { promise, resolve } = Promise.withResolvers<boolean>()

    settlePending(false)
    exited.current = resolve
    setInstant(true)
    latest.current.onClose()
    closeTimer.current = window.setTimeout(() => {
      closeTimer.current = null
      setInstant(false)
      settlePending(false)
    }, CLOSE_WAIT_MS)

    return promise
  }, [settlePending])

  const handle = useMemo<SheetHandle>(() => ({ requestClose, closeNow }), [requestClose, closeNow])

  useEffect(() => registerSheet(id, handle), [id, handle])

  useEffect(() => () => settlePending(false), [settlePending])

  const onExitTransitionEnd = (): void => {
    settlePending(true)
    setInstant(false)
  }

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

  const body = (
    <ErrorBoundary scope="feature" name="Dialog content" resetKeys={[opened]}>
      {children}
    </ErrorBoundary>
  )

  let footerPaddingBottom: string | undefined

  if (mobileShell) {
    footerPaddingBottom = 'calc(var(--mantine-spacing-md) + var(--sheet-pb, 0px))'
  } else if (isMobile) {
    footerPaddingBottom = 'calc(var(--mantine-spacing-md) + var(--safe-bottom))'
  }

  const content = (
    <>
      {disableScrollArea ? (
        <Box style={{ ...columnStyle, flex: 1, padding, paddingBottom: bottomInset }}>{body}</Box>
      ) : (
        <FillScrollArea
          type={mobileShell ? 'scroll' : 'auto'}
          {...(mobileShell ? { scrollbarSize: 3, scrollHideDelay: 200 } : {})}
        >
          <Box p={padding} {...(bottomInset === undefined ? {} : { pb: bottomInset })}>
            {body}
          </Box>
        </FillScrollArea>
      )}
      {hasFooter && (
        <Box
          className={mobileShell ? iosClasses.sheetFooter : undefined}
          px="lg"
          py="md"
          style={{
            flexShrink: 0,
            borderTop: '1px solid var(--mantine-color-default-border)',
            paddingBottom: footerPaddingBottom,
          }}
        >
          {typeof footer === 'function' ? footer(requestClose) : footer}
        </Box>
      )}
    </>
  )

  if (mobileShell) {
    const detentList = detents && detents.length > 0 ? detents : undefined
    let startDetent = 'auto'

    if (detentList) {
      startDetent = detentList.includes('medium') ? 'medium' : 'large'
    } else if (fullHeight) {
      startDetent = 'large'
    }

    let duration = 320

    if (instant) {
      duration = 0
    } else if (reducedMotion) {
      duration = 200
    }

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
          duration,
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
          <SheetHeader
            title={title}
            backAction={backAction}
            primaryAction={primaryAction}
            busy={busy}
            loading={loading}
            showCancel={showCancel}
            showDone={showDone}
            onClose={requestClose}
          />
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
