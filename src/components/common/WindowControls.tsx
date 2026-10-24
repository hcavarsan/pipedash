import { ActionIcon, Box, Group, UnstyledButton } from '@mantine/core'
import { IconArrowsDiagonal, IconMinus, IconSquare, IconSquares, IconX } from '@tabler/icons-react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { type CSSProperties, type ReactNode, useEffect, useState } from 'react'

import { logger } from '../../lib/logger'
import { type Platform, platform } from '../../utils/platform'

import classes from './WindowControls.module.css'

const TRAFFIC_LIGHTS = {
  close: { color: '#FF5F57', hoverColor: '#FF4136' },
  minimize: { color: '#FEBC2E', hoverColor: '#F0A000' },
  maximize: { color: '#28C840', hoverColor: '#1FB032' },
} as const

interface TrafficLightProps {
  kind: keyof typeof TRAFFIC_LIGHTS
  label: string
  onClick: () => void
  icon: ReactNode
}

const TrafficLight = ({ kind, label, onClick, icon }: TrafficLightProps) => (
  <UnstyledButton
    type="button"
    className={classes.trafficLight}
    aria-label={label}
    onClick={onClick}
    style={
      {
        '--traffic-light-color': TRAFFIC_LIGHTS[kind].color,
        '--traffic-light-hover-color': TRAFFIC_LIGHTS[kind].hoverColor,
      } as CSSProperties
    }
  >
    <span className={classes.trafficLightIcon} aria-hidden>
      {icon}
    </span>
  </UnstyledButton>
)

export const WindowControls = () => {
  const [currentPlatform, setCurrentPlatform] = useState<Platform | null>(null)
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
        logger.error('WindowControls', 'Error initializing window controls', err)
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
      logger.error('WindowControls', 'Error minimizing window', err)
    }
  }

  const handleMaximize = async () => {
    try {
      const appWindow = getCurrentWindow()

      await appWindow.toggleMaximize()
      setIsMaximized((prev) => !prev)
    } catch (err) {
      logger.error('WindowControls', 'Error maximizing window', err)
    }
  }

  const handleClose = async () => {
    try {
      const appWindow = getCurrentWindow()

      await appWindow.close()
    } catch (err) {
      logger.error('WindowControls', 'Error closing window', err)
    }
  }

  if (!currentPlatform) {
    return <Box w={60} h="100%" />
  }

  if (currentPlatform === 'macos') {
    return (
      <Group gap="xs" pl="md" h="100%" align="center" style={{ WebkitAppRegion: 'no-drag' }}>
        <TrafficLight kind="close" label="Close window" onClick={handleClose} icon={<IconX size={8} stroke={3} />} />
        <TrafficLight
          kind="minimize"
          label="Minimize window"
          onClick={handleMinimize}
          icon={<IconMinus size={8} stroke={3} />}
        />
        <TrafficLight
          kind="maximize"
          label={isMaximized ? 'Restore window' : 'Maximize window'}
          onClick={handleMaximize}
          icon={<IconArrowsDiagonal size={8} stroke={3} />}
        />
      </Group>
    )
  }

  return (
    <Group gap={0} h="100%" style={{ WebkitAppRegion: 'no-drag' }}>
      <ActionIcon
        variant="subtle"
        color="gray"
        h="100%"
        w={46}
        radius={0}
        onClick={handleMinimize}
        aria-label="Minimize window"
      >
        <IconMinus size={14} stroke={1.5} />
      </ActionIcon>

      <ActionIcon
        variant="subtle"
        color="gray"
        h="100%"
        w={46}
        radius={0}
        onClick={handleMaximize}
        aria-label={isMaximized ? 'Restore window' : 'Maximize window'}
      >
        {isMaximized ? <IconSquares size={14} stroke={1.5} /> : <IconSquare size={14} stroke={1.5} />}
      </ActionIcon>

      <ActionIcon
        variant="subtle"
        color="red"
        h="100%"
        w={46}
        radius={0}
        onClick={handleClose}
        aria-label="Close window"
      >
        <IconX size={14} stroke={1.5} />
      </ActionIcon>
    </Group>
  )
}
