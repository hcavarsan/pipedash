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
      size={800}
      closeOnClickOutside={!loading}
      closeOnEscape={!loading}
      withCloseButton={!loading}
      styles={{
        title: {
          fontWeight: 600,
          fontSize: '1.25rem',
        },
      }}
    >
      {children}
    </Modal>
  )
}
