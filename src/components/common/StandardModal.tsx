import { Box, Drawer, Modal, ScrollArea } from '@mantine/core'
import type { ReactNode } from 'react'

import { useIsMobile } from '../../hooks/useIsMobile'
import type { ModalBaseProps } from '../../types'

type ModalSize = 'md' | 'lg' | 'xl'

interface StandardModalProps extends ModalBaseProps {
  title: ReactNode
  children: ReactNode
  footer?: ReactNode
  loading?: boolean
  closable?: boolean
  size?: ModalSize
  contentPadding?: boolean
  disableScrollArea?: boolean
}

const DESKTOP_LAYOUT: Record<ModalSize, { width: string; height?: string }> = {
  md: { width: 'min(560px, 92vw)' },
  lg: { width: 'min(880px, 92vw)' },
  xl: { width: 'min(1400px, 95vw)', height: 'min(90vh, 1000px)' },
}

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
}: StandardModalProps) => {
  const { isMobile } = useIsMobile()
  const padding = contentPadding ? 'var(--mantine-spacing-lg)' : 0

  const sharedProps = {
    opened,
    onClose,
    title,
    closeOnClickOutside: closable && !loading,
    closeOnEscape: closable && !loading,
    withCloseButton: closable && !loading,
    closeButtonProps: { 'aria-label': 'Close dialog' },
    zIndex: 300,
  }

  const sharedStyles = {
    title: { fontWeight: 600, fontSize: 'var(--mantine-font-size-lg)' },
    header: {
      borderBottom: '1px solid var(--mantine-color-default-border)',
      paddingInline: 'var(--mantine-spacing-lg)',
    },
    body: { ...columnStyle, flex: 1, padding: 0 },
  }

  const content = (
    <>
      {disableScrollArea ? (
        <Box style={{ ...columnStyle, flex: 1, padding }}>{children}</Box>
      ) : (
        <ScrollArea type="auto" style={{ flex: 1, minHeight: 0 }}>
          <Box p={padding}>{children}</Box>
        </ScrollArea>
      )}
      {footer && (
        <Box
          px="lg"
          py="md"
          style={{
            flexShrink: 0,
            borderTop: '1px solid var(--mantine-color-default-border)',
            paddingBottom: isMobile ? 'calc(var(--mantine-spacing-md) + var(--safe-bottom))' : undefined,
          }}
        >
          {footer}
        </Box>
      )}
    </>
  )

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
