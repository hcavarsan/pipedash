import { ActionIcon, Button, Card, Group, Stack, Text, Tooltip } from '@mantine/core'
import { IconPlus, IconTrash } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'

import { EmptyState } from '@/components/common/EmptyState'
import { ErrorState } from '@/components/common/ErrorState'
import { LoadingState } from '@/components/common/LoadingState'
import { IosGroup, IosRow } from '@/components/ios/IosList'
import {
  usePairedDevices,
  useRemovePairedDevice,
  useSharingStatus,
} from '@/features/connection/queries/useConnectionQueries'
import { useMobileShell } from '@/hooks/useIsMobile'
import { useNow } from '@/hooks/useNow'
import type { PairedDevice } from '@/types/native'
import { openConfirm } from '@/utils/confirm'
import { formatRelativeTime } from '@/utils/formatRelativeTime'

import { AddDeviceModal } from './AddDeviceModal'

const DESCRIPTION = 'Phones and computers that show the pipelines from here.'

const PLATFORM_LABELS: Record<PairedDevice['platform'], string> = {
  macos: 'macOS',
  windows: 'Windows',
  linux: 'Linux',
  ios: 'iOS',
  android: 'Android',
}

const DATE_FORMAT = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' })

interface PairedDevicesCardProps {
  sharing: boolean
}

interface AddDeviceButtonProps {
  blocked: boolean
  onClick: () => void
  variant: 'default' | 'light'
}

function AddDeviceButton({ blocked, onClick, variant }: AddDeviceButtonProps): ReactNode {
  return (
    <Tooltip label="Turn on sharing first" disabled={!blocked}>
      <Button
        variant={variant}
        size="xs"
        leftSection={<IconPlus size={14} aria-hidden />}
        {...(blocked ? { 'aria-disabled': true, 'data-disabled': true } : {})}
        onClick={() => {
          if (!blocked) {
            onClick()
          }
        }}
      >
        Add device
      </Button>
    </Tooltip>
  )
}

function deviceDetails(device: PairedDevice, now: number): string {
  const paired = DATE_FORMAT.format(new Date(device.created_at))
  const seen = device.last_seen_at
    ? `Last connected ${formatRelativeTime(device.last_seen_at, now)}`
    : 'Never connected'

  return `${PLATFORM_LABELS[device.platform]} · Paired ${paired} · ${seen}${device.push ? ' · Push alerts on' : ''}`
}
interface PairedDeviceRowProps {
  device: PairedDevice
  now: number
  actions: ReactNode
}

function PairedDeviceRow({ device, now, actions }: PairedDeviceRowProps): ReactNode {
  return (
    <Group justify="space-between" wrap="nowrap" gap="sm">
      <Stack gap={0} miw={0}>
        <Group gap="xs" wrap="nowrap">
          <Text size="sm" fw={500} truncate>
            {device.name}
          </Text>
          {device.current && (
            <Text size="xs" c="dimmed">
              This device
            </Text>
          )}
        </Group>
        <Text size="xs" c="dimmed" truncate>
          {deviceDetails(device, now)}
        </Text>
      </Stack>
      {actions}
    </Group>
  )
}

export function PairedDevicesCard({ sharing }: PairedDevicesCardProps): ReactNode {
  const mobileShell = useMobileShell()
  const now = useNow(60_000)
  const [adding, setAdding] = useState(false)
  const { data, error, isLoading, isRefetching, refetch } = usePairedDevices({ enabled: true })
  const remove = useRemovePairedDevice()
  const sharingStatus = useSharingStatus({ enabled: sharing })
  const blocked = sharing && sharingStatus.data?.running !== true

  const confirmRemove = (device: PairedDevice): void => {
    openConfirm({
      title: `Remove ${device.name}?`,
      children: `${device.name} loses access right away.`,
      labels: { confirm: 'Remove', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: () => remove.mutate({ id: device.id, name: device.name }),
    })
  }

  const openAdd = (): void => setAdding(true)

  const removeButton = (device: PairedDevice): ReactNode => {
    const label = `Remove ${device.name}`
    const button = (
      <ActionIcon
        variant="subtle"
        color="red"
        size={mobileShell ? 'xl' : 'lg'}
        aria-label={label}
        loading={remove.isPending && remove.variables?.id === device.id}
        onClick={() => confirmRemove(device)}
      >
        <IconTrash size={mobileShell ? 20 : 16} aria-hidden />
      </ActionIcon>
    )

    return mobileShell ? button : <Tooltip label={label}>{button}</Tooltip>
  }

  const modal = <AddDeviceModal opened={adding} onClose={() => setAdding(false)} />

  let state: ReactNode = null
  if (error) {
    state = (
      <ErrorState
        title="Couldn't load paired devices"
        error={error}
        onRetry={() => refetch()}
        retrying={isRefetching}
      />
    )
  } else if (isLoading || !data) {
    state = <LoadingState message="Loading paired devices" />
  } else if (data.length === 0) {
    state = (
      <EmptyState
        title="No paired devices"
        description="Add a phone or computer to show these pipelines on it."
        action={<AddDeviceButton blocked={blocked} onClick={openAdd} variant="default" />}
      />
    )
  }

  const mobileRows = data?.map((device) => (
    <IosRow
      key={device.id}
      title={device.name}
      subtitle={deviceDetails(device, now)}
      value={device.current ? 'This device' : undefined}
      accessory={device.removable ? removeButton(device) : undefined}
    />
  ))

  const desktopRows = data?.map((device) => (
    <PairedDeviceRow key={device.id} device={device} now={now} actions={device.removable && removeButton(device)} />
  ))

  if (mobileShell) {
    return (
      <>
        {state ?? (
          <IosGroup
            header="Paired devices"
            headerAction={<AddDeviceButton blocked={blocked} onClick={openAdd} variant="light" />}
            footer={DESCRIPTION}
          >
            {mobileRows}
          </IosGroup>
        )}
        {modal}
      </>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <Group justify="space-between" wrap="nowrap" gap="sm" align="flex-start">
          <Stack gap={0}>
            <Text size="sm" fw={600}>
              Paired devices
            </Text>
            <Text size="xs" c="dimmed">
              {DESCRIPTION}
            </Text>
          </Stack>
          {data && data.length > 0 && <AddDeviceButton blocked={blocked} onClick={openAdd} variant="default" />}
        </Group>

        {state ?? <Stack gap="sm">{desktopRows}</Stack>}
      </Stack>
      {modal}
    </Card>
  )
}
