import { Button, Stack, Text, ThemeIcon, Title } from '@mantine/core'
import { IconCircleCheck } from '@tabler/icons-react'
import { useQueryClient } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { useNavigate } from 'react-router-dom'

import { withBasePath } from '../../../lib/basePath'
import { queryKeys } from '../../../lib/queryKeys'

interface ConnectServerDoneProps {
  host: string
  alerts: boolean
  mobileShell: boolean
}

export function ConnectServerDone({ host, alerts, mobileShell }: ConnectServerDoneProps): ReactNode {
  const navigate = useNavigate()
  const queryClient = useQueryClient()

  const done = alerts
    ? {
        title: `Alerts from ${host} are on`,
        text: 'This device keeps its own pipelines and also gets alerts from that server.',
        label: 'Done',
        onClick: () => {
          void queryClient.invalidateQueries({ queryKey: queryKeys.localNotifications.settings() })
          void navigate('/settings/notifications', { replace: true })
        },
      }
    : {
        title: `Connected to ${host}`,
        text: `This device now shows the pipelines from ${host}.`,
        label: 'Open dashboard',
        onClick: () => window.location.replace(withBasePath('/pipelines')),
      }

  return (
    <Stack align="center" gap="md" ta="center">
      <ThemeIcon variant="transparent" color="green" size={48}>
        <IconCircleCheck size={24} aria-hidden />
      </ThemeIcon>
      <Stack gap="2xs" align="center">
        <Title order={2} size="h3">
          {done.title}
        </Title>
        <Text size="sm" c="dimmed" style={{ overflowWrap: 'anywhere' }}>
          {done.text}
        </Text>
      </Stack>
      <Button fullWidth size={mobileShell ? 'lg' : 'md'} onClick={done.onClick}>
        {done.label}
      </Button>
    </Stack>
  )
}
