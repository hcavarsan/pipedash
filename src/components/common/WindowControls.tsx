import { ActionIcon, Box, Group, Tooltip, UnstyledButton } from '@mantine/core'
import { IconArrowsDiagonal, IconMinus, IconSquare, IconSquares, IconX } from '@tabler/icons-react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { type ReactNode, useEffect, useState } from 'react'

import { logger } from '../../lib/logger'
import { type Platform, platform } from '../../utils/platform'

import classes from './WindowControls.module.css'

interface TrafficLightProps {
  kind: 'close' | 'minimize' | 'maximize'
  label: string
  onClick: () => void
  icon: ReactNode
}

const TrafficLight = ({ kind, label, onClick, icon }: TrafficLightProps) => (
  <UnstyledButton type="button" className={classes.trafficLight} aria-label={label} onClick={onClick} data-kind={kind}>
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
    let unlisten: (() => void) | undefined

    const init = async () => {
      try {
        const p = await platform()

        if (!isMounted) {
          return
        }
        setCurrentPlatform(p)

        const appWindow = getCurrentWindow()
        const syncMaximized = async () => {
          const maximized = await appWindow.isMaximized()

          if (isMounted) {
            setIsMaximized(maximized)
          }
        }

        unlisten = await appWindow.onResized(() => {
          syncMaximized().catch((error: unknown) =>
            logger.error('WindowControls', 'Could not read window state', error)
          )
        })
        if (!isMounted) {
          unlisten()
          return
        }
        await syncMaximized()
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
      unlisten?.()
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
      setIsMaximized(await appWindow.isMaximized())
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

  const maximizeLabel = isMaximized ? 'Restore window' : 'Maximize window'

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
          label={maximizeLabel}
          onClick={handleMaximize}
          icon={<IconArrowsDiagonal size={8} stroke={3} />}
        />
      </Group>
    )
  }

  return (
    <Group gap={0} h="100%" style={{ WebkitAppRegion: 'no-drag' }}>
      <Tooltip label="Minimize window">
        <ActionIcon h="100%" w={46} radius={0} onClick={handleMinimize} aria-label="Minimize window">
          <IconMinus size={14} stroke={1.5} />
        </ActionIcon>
      </Tooltip>

      <Tooltip label={maximizeLabel}>
        <ActionIcon h="100%" w={46} radius={0} onClick={handleMaximize} aria-label={maximizeLabel}>
          {isMaximized ? <IconSquares size={14} stroke={1.5} /> : <IconSquare size={14} stroke={1.5} />}
        </ActionIcon>
      </Tooltip>

      <Tooltip label="Close window">
        <ActionIcon color="red" h="100%" w={46} radius={0} onClick={handleClose} aria-label="Close window">
          <IconX size={14} stroke={1.5} />
        </ActionIcon>
      </Tooltip>
    </Group>
  )
}
