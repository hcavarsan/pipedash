import React from 'react'

import { ActionIcon } from '@mantine/core'
import { IconPlus } from '@tabler/icons-react'

import { useIsMobile } from '../../hooks/useIsMobile'

interface MobileFooterProps {
  onAddProvider: () => void;
}

export const MobileFooter = React.memo(({ onAddProvider }: MobileFooterProps) => {
  const { isMobile } = useIsMobile()

  if (!isMobile) {
    return null
  }

  return (
    <ActionIcon
      size={64}
      radius="xl"
      variant="light"
      color="blue"
      onClick={onAddProvider}
      aria-label="Add Provider"
      style={{
        position: 'fixed',
        bottom: 'calc(24px + env(safe-area-inset-bottom))',
        left: 24,
        zIndex: 200,
        backgroundColor: 'rgba(34, 139, 230, 0.35)',
      }}
    >
      <IconPlus size={28} />
    </ActionIcon>
  )
})
