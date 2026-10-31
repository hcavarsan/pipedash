import { Box, Button } from '@mantine/core'
import { IconDownload } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { Link, useLocation } from 'react-router-dom'

import { useUpdateStatus } from '../../features/settings/queries/useUpdateQueries'
import { dialogOpen } from '../../lib/overlays'
import { isDesktopTauri } from '../../utils/platform'

export function DesktopUpdateIndicator(): ReactNode {
  return isDesktopTauri() ? <NativeDesktopUpdateIndicator /> : null
}

function NativeDesktopUpdateIndicator(): ReactNode {
  const { pathname } = useLocation()
  const status = useUpdateStatus().data
  if (pathname.replace(/\/$/, '') === '/settings/general' || !status?.available_version) {
    return null
  }
  const label = status.phase === 'ready' ? 'Restart to update' : 'Update available'
  return (
    <Button
      component={Link}
      to="/settings/general"
      variant="subtle"
      color="yellow"
      size="compact-sm"
      leftSection={<IconDownload size={16} aria-hidden />}
      aria-label={label}
      onClick={(event) => {
        if (dialogOpen()) {
          event.preventDefault()
        }
      }}
    >
      <Box component="span" visibleFrom="sm">
        {label}
      </Box>
      <Box component="span" hiddenFrom="sm">
        Update
      </Box>
    </Button>
  )
}
