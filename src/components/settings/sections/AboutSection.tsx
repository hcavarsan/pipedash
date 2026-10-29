import { Button, Card, Divider, Stack, Text } from '@mantine/core'
import { IconExternalLink } from '@tabler/icons-react'
import { useQuery } from '@tanstack/react-query'
import { getVersion } from '@tauri-apps/api/app'
import type { ReactNode } from 'react'

import { useMobileShell } from '@/hooks/useIsMobile'
import { logger } from '@/lib/logger'
import { queryKeys } from '@/lib/queryKeys'
import { apiService, isTauri } from '@/services'
import { displayErrorNotification } from '@/utils/errorDisplay'
import { openExternal } from '@/utils/openExternal'

import { IosGroup, IosRow } from '../../ios/IosList'

const LINKS = [
  { label: 'Privacy Policy', url: 'https://pipedash.app/privacy' },
  { label: 'Support', url: 'https://pipedash.app/support' },
  { label: 'Source code', url: 'https://github.com/hcavarsan/pipedash' },
]

export function AboutSection(): ReactNode {
  const mobileShell = useMobileShell()
  const { data: version } = useQuery({
    queryKey: queryKeys.app.version(),
    queryFn: () => (isTauri() ? getVersion() : apiService.getServerVersion()),
    staleTime: Infinity,
  })

  const openLink = (label: string, url: string) =>
    openExternal(url).catch((error: unknown) => {
      logger.error('AboutSection', `Couldn't open ${url}`, error)
      displayErrorNotification(error, `Couldn't open ${label}`)
    })

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        <IosGroup>
          <IosRow title="Pipedash" value={`Version ${version ?? 'unknown'}`} />
        </IosGroup>
        <IosGroup>
          {LINKS.map(({ label, url }) => (
            <IosRow
              key={url}
              title={label}
              accessory={<IconExternalLink size={18} aria-hidden />}
              onClick={() => {
                void openLink(label, url)
              }}
            />
          ))}
        </IosGroup>
      </Stack>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <Stack gap={0}>
          <Text size="sm" fw={600}>
            Pipedash
          </Text>
          <Text size="xs" c="dimmed">
            Version {version ?? 'unknown'}
          </Text>
        </Stack>

        <Divider />

        <Stack gap="xs">
          {LINKS.map(({ label, url }) => (
            <Button
              key={url}
              component="a"
              href={url}
              variant="default"
              justify="space-between"
              rightSection={<IconExternalLink size={16} />}
              onClick={(event: React.MouseEvent) => {
                event.preventDefault()
                void openLink(label, url)
              }}
            >
              {label}
            </Button>
          ))}
        </Stack>
      </Stack>
    </Card>
  )
}
