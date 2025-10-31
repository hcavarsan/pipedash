import { useEffect, useState } from 'react'

import { ActionIcon, Box, Group } from '@mantine/core'
import { IconMinus, IconSquare, IconSquareCheck, IconX } from '@tabler/icons-react'
import { getCurrentWindow } from '@tauri-apps/api/window'

import { platform } from '../../utils/platform'

interface MacOSButtonProps {
  color: string;
  hoverColor: string;
  onClick: () => void;
  icon?: React.ReactNode;
}

const MacOSButton = ({ color, hoverColor, onClick, icon }: MacOSButtonProps) => {
  const [isHovered, setIsHovered] = useState(false)

  return (
    <Box
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      onClick={onClick}
      style={{
        width: 12,
        height: 12,
        borderRadius: '50%',
        backgroundColor: color,
        cursor: 'pointer',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        fontSize: 8,
        fontWeight: 'bold',
        color: 'rgba(0, 0, 0, 0.7)',
        transition: 'background-color 0.15s ease',
        border: '0.5px solid rgba(0, 0, 0, 0.15)',
      }}
      onMouseOver={(e) => {
        e.currentTarget.style.backgroundColor = hoverColor
      }}
      onMouseOut={(e) => {
        e.currentTarget.style.backgroundColor = color
      }}
    >
      {isHovered && icon}
    </Box>
  )
}

export const WindowControls = () => {
  const [currentPlatform, setCurrentPlatform] = useState<'macos' | 'windows' | 'linux' | null>(null)
  const [isMaximized, setIsMaximized] = useState(false)

  useEffect(() => {
    let isMounted = true

    const init = async () => {
      try {
        const p = await platform()


        if (!isMounted) {
return
}
        setCurrentPlatform(p)

        const appWindow = getCurrentWindow()
        const maximized = await appWindow.isMaximized()


        if (!isMounted) {
return
}
        setIsMaximized(maximized)
      } catch (err) {
        console.error('Error initializing window controls:', err)
        if (isMounted) {
          setCurrentPlatform('windows')
        }
      }
    }

    init()

    return () => {
      isMounted = false
    }
  }, [])

  const handleMinimize = async () => {
    try {
      const appWindow = getCurrentWindow()


      await appWindow.minimize()
    } catch (err) {
      console.error('Error minimizing window:', err)
    }
  }

  const handleMaximize = async () => {
    try {
      const appWindow = getCurrentWindow()


      await appWindow.toggleMaximize()
      setIsMaximized((prev) => !prev)
    } catch (err) {
      console.error('Error maximizing window:', err)
    }
  }

  const handleClose = async () => {
    try {
      const appWindow = getCurrentWindow()


      await appWindow.close()
    } catch (err) {
      console.error('Error closing window:', err)
    }
  }

  if (!currentPlatform) {
    return <Box style={{ width: 60, height: '100%' }} />
  }

  if (currentPlatform === 'macos') {
    return (
      <Group
        gap={8}
        pl={16}
        pr={0}
        h="100%"
        align="center"
        style={{
          WebkitAppRegion: 'no-drag',
        }}
      >
        {/* Close - Red */}
        <MacOSButton
          color="#FF5F57"
          hoverColor="#FF4136"
          onClick={handleClose}
          icon={<span style={{ fontSize: 9 }}>✕</span>}
        />

        {/* Minimize - Yellow */}
        <MacOSButton
          color="#FEBC2E"
          hoverColor="#F0A000"
          onClick={handleMinimize}
          icon={<span style={{ fontSize: 9, marginBottom: 2 }}>−</span>}
        />

        {/* Maximize - Green */}
        <MacOSButton
          color="#28C840"
          hoverColor="#1FB032"
          onClick={handleMaximize}
          icon={
            <svg width="8" height="8" viewBox="0 0 8 8" fill="none">
              <path
                d="M1 3L4 6L7 3"
                stroke="rgba(0,0,0,0.7)"
                strokeWidth="1"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          }
        />
      </Group>
    )
  }

  return (
    <Group
      gap={0}
      h="100%"
      pl={0}
      pr={0}
      style={{
        WebkitAppRegion: 'no-drag',
      }}
    >
      {/* Minimize */}
      <ActionIcon
        variant="subtle"
        color="gray"
        h="100%"
        w={46}
        radius={0}
        onClick={handleMinimize}
        styles={{
          root: {
            '&:hover': {
              backgroundColor: 'rgba(255, 255, 255, 0.05)',
            },
          },
        }}
      >
        <IconMinus size={14} stroke={1.5} />
      </ActionIcon>

      {/* Maximize/Restore */}
      <ActionIcon
        variant="subtle"
        color="gray"
        h="100%"
        w={46}
        radius={0}
        onClick={handleMaximize}
        styles={{
          root: {
            '&:hover': {
              backgroundColor: 'rgba(255, 255, 255, 0.05)',
            },
          },
        }}
      >
        {isMaximized ? (
          <IconSquareCheck size={14} stroke={1.5} />
        ) : (
          <IconSquare size={14} stroke={1.5} />
        )}
      </ActionIcon>

      {/* Close */}
      <ActionIcon
        variant="subtle"
        color="gray"
        h="100%"
        w={46}
        radius={0}
        onClick={handleClose}
        styles={{
          root: {
            '&:hover': {
              backgroundColor: '#e81123',
              color: 'white',
            },
          },
        }}
      >
        <IconX size={14} stroke={1.5} />
      </ActionIcon>
    </Group>
  )
}
