import { ReactNode } from 'react'

import { Modal } from '@mantine/core'

interface StandardModalProps {
  opened: boolean;
  onClose: () => void;
  title: ReactNode;
  children: ReactNode;
  loading?: boolean;
}


export const StandardModal = ({
  opened,
  onClose,
  title,
  children,
  loading = false,
}: StandardModalProps) => {
  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title={title}
      size="xl"
      closeOnClickOutside={!loading}
      closeOnEscape={!loading}
      withCloseButton={!loading}
      styles={{
        title: {
          fontWeight: 600,
          fontSize: '1.25rem',
        },
        header: {
          borderBottom: '1px solid var(--mantine-color-default-border)',
          paddingBottom: '1.25rem',
          marginBottom: '1.5rem',
        },
        body: {
          display: 'flex',
          flexDirection: 'column',
          height: '70vh',
          maxHeight: '70vh',
        },
      }}
    >
      {children}
    </Modal>
  )
}
