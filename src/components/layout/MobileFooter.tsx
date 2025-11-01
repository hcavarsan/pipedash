import React from 'react'

import { Box, Group, Text } from '@mantine/core'
import { IconPlus } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'

interface MobileFooterProps {
  onAddProvider: () => void;
}

export const MobileFooter = React.memo(({ onAddProvider }: MobileFooterProps) => {
  const isMobile = useIsMobile()

  // Don't render on desktop
  if (!isMobile) {
    return null
  }

  return (
    <Box
      onClick={onAddProvider}
      style={{
        position: 'fixed',
        bottom: 0,
        left: 0,
        right: 0,
        zIndex: 200,
        backgroundColor: 'var(--mantine-color-body)',
        borderTop: '1px solid var(--mantine-color-default-border)',
        boxShadow: '0 -2px 8px rgba(0, 0, 0, 0.1)',
        paddingLeft: '16px',
        paddingRight: '16px',
        paddingTop: '12px',
        paddingBottom: 'calc(8px + env(safe-area-inset-bottom))',
        cursor: 'pointer',
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'center',
        transition: 'background-color 0.15s ease',
        height: 'auto',
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.backgroundColor = 'var(--mantine-color-default-hover)'
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.backgroundColor = 'var(--mantine-color-body)'
      }}
    >
      <Group gap="6px">
        <IconPlus size={16} style={{ color: 'var(--mantine-color-blue-6)' }} />
        <Text size="sm" fw={500} c="blue">
          Add Provider
        </Text>
      </Group>
    </Box>
  )
})
