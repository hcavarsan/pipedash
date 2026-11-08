import { ActionIcon, Group, Loader, Menu, Stack, Text, Tooltip } from '@mantine/core'
import { IconInfoCircle } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { ToneDot } from '../../features/pipelines/components/ToneDot'
import { useNow } from '../../hooks/useNow'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import type { TableRow } from '../../types/pipelineRows'
import type { ProviderFreshness, ProviderSummary } from '../../types/providers'
import { describeFetch, freshnessTone, providerTone, TONE_META } from '../../utils/fetchInfo'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'

export function RepositoryFetchIndicator({ row }: { row: TableRow }): ReactNode {
  if (row.type !== 'section' || !row.hasPendingFetch) {
    return null
  }

  return (
    <Tooltip label="Fetching pipelines…">
      <Loader size="sm" aria-label="Fetching pipelines" />
    </Tooltip>
  )
}

function ProviderFetchLines({ provider, now }: { provider: ProviderSummary; now: number }): ReactNode {
  const info = describeFetch(provider, now)
  const tone = providerTone(provider, now)

  return (
    <Stack gap={0}>
      <Group gap="xs" wrap="nowrap">
        <ToneDot tone={tone} />
        <Text size="xs" fw={600}>
          {info.name} · {TONE_META[tone].label}
        </Text>
      </Group>
      <Text size="xs">Last fetch: {info.lastFetch}</Text>
      <Text size="xs">Status: {info.status}</Text>
    </Stack>
  )
}

export function FetchInfo({
  providers,
  freshness,
}: {
  providers: ProviderSummary[]
  freshness?: ProviderFreshness | undefined
}): ReactNode {
  const now = useNow(30_000)
  const openNotifications = useNotificationCenterStore((state) => state.open)
  const [only] = providers
  const source = freshness ?? only?.freshness

  if (!only || !source) {
    return null
  }

  const worst = freshnessTone(source, now)
  const openProvider = (provider: ProviderSummary) => openNotifications({ type: 'provider', providerId: provider.id })
  const button = (
    <ActionIcon
      variant="transparent"
      color="gray"
      c={worst === 'fresh' ? 'dimmed' : TONE_META[worst].color}
      size="sm"
      aria-label="Fetch info"
      onClick={providers.length === 1 ? () => openProvider(only) : undefined}
    >
      <IconInfoCircle size={16} aria-hidden />
    </ActionIcon>
  )
  const icon = (
    <Tooltip
      multiline
      maw={420}
      events={{ hover: true, focus: true, touch: false }}
      label={
        <Stack gap="xs">
          {providers.map((provider) => (
            <ProviderFetchLines key={provider.id} provider={provider} now={now} />
          ))}
        </Stack>
      }
    >
      {providers.length < 2 ? button : <Menu.Target>{button}</Menu.Target>}
    </Tooltip>
  )

  if (providers.length < 2) {
    return icon
  }

  return (
    <Menu position="bottom-end">
      {icon}
      <Menu.Dropdown>
        {providers.map((provider) => (
          <Menu.Item
            key={provider.id}
            leftSection={<ToneDot tone={providerTone(provider, now)} />}
            onClick={() => openProvider(provider)}
          >
            {provider.name}
          </Menu.Item>
        ))}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
}

export function RowFetchInfo({
  row,
  getProvider,
}: {
  row: TableRow
  getProvider: (providerId: number) => ProviderSummary | undefined
}): ReactNode {
  return (
    <FetchInfo
      providers={Array.from(row.providerIds, getProvider).flatMap((p) => p ?? [])}
      freshness={row.type === 'section' ? row.freshness : undefined}
    />
  )
}
