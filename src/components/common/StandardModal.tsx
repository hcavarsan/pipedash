import { ReactNode } from 'react'

import { Box, Drawer, Modal, ScrollArea } from '@mantine/core'

import { useIsMobile } from '../../hooks/useIsMobile'
import type { ModalBaseProps } from '../../types'

interface StandardModalProps extends ModalBaseProps {
  title: ReactNode;
  children: ReactNode;
  loading?: boolean;
  disableAspectRatio?: boolean;
  footer?: ReactNode;
  contentPadding?: boolean;
  disableScrollArea?: boolean;
  radius?: 'xs' | 'sm' | 'md' | 'lg' | 'xl' | number;
}

const SHARED_STYLES = {
  title: {
    fontWeight: 600,
    fontSize: '1.25rem',
  },
  header: {
    borderBottom: '1px solid var(--mantine-color-default-border)',
    paddingBottom: '1.25rem',
  },
  body: {
    display: 'flex',
    flexDirection: 'column' as const,
    flex: 1,
    minHeight: 0,
    padding: 0,
    overflow: 'hidden',
  },
}

export const StandardModal = ({
  opened,
  onClose,
  title,
  children,
  loading = false,
  disableAspectRatio = false,
  footer,
  contentPadding = true,
  disableScrollArea = false,
  radius = 'md',
}: StandardModalProps) => {
  const { isMobile, isDesktop } = useIsMobile()

  const sharedProps = {
    opened,
    onClose,
    title,
    closeOnClickOutside: !loading,
    closeOnEscape: !loading,
    withCloseButton: !loading,
    zIndex: 300,
  }

  const renderContent = () => (
    <>
      {disableScrollArea ? (
        <Box style={{
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
          padding: contentPadding ? 'var(--mantine-spacing-md)' : 0,
          paddingTop: contentPadding ? 'var(--mantine-spacing-xl)' : 0,
        }}>
          {children}
        </Box>
      ) : (
        <ScrollArea
          style={{ flex: 1, minHeight: 0 }}
          type="auto"
          styles={{
            root: { minHeight: 0 },
            viewport: {
              padding: contentPadding ? 'var(--mantine-spacing-md)' : 0,
              paddingTop: contentPadding ? 'var(--mantine-spacing-xl)' : 0,
            },
          }}
        >
          {children}
        </ScrollArea>
      )}

      {footer && (
        <Box
          style={{
            borderTop: '1px solid var(--mantine-color-default-border)',
            padding: isDesktop ? '12px 16px' : '8px 16px',
            paddingBottom: isMobile
              ? 'calc(8px + env(safe-area-inset-bottom))'
              : '12px',
            flexShrink: 0,
            backgroundColor: 'var(--mantine-color-dark-8)',
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
          title: SHARED_STYLES.title,
          header: {
            ...SHARED_STYLES.header,
            paddingTop: 'calc(var(--mantine-spacing-md) + env(safe-area-inset-top))',
          },
          body: SHARED_STYLES.body,
          content: {
            display: 'flex',
            flexDirection: 'column',
            backgroundColor: 'var(--mantine-color-dark-8)',
            borderTopLeftRadius: `var(--mantine-radius-${radius})`,
            borderTopRightRadius: `var(--mantine-radius-${radius})`,
          },
        }}
      >
        {renderContent()}
      </Drawer>
    )
  }

  const modalSize = !disableAspectRatio ? 'min(80vh, 80vw)' : '95vw'

  return (
    <Modal
      {...sharedProps}
      size={modalSize}
      centered
      radius={radius}
      styles={{
        title: SHARED_STYLES.title,
        header: {
          ...SHARED_STYLES.header,
          flexShrink: 0,
        },
        body: SHARED_STYLES.body,
        inner: {
          padding: 'var(--mantine-spacing-md)',
        },
        content: !disableAspectRatio ? {
          aspectRatio: '1 / 1',
          height: '80vh',
          maxHeight: '80vh',
          maxWidth: '80vw',
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
        } : {
          height: '90vh',
          maxHeight: '90vh',
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
        },
      }}
    >
      {renderContent()}
    </Modal>
  )
}
