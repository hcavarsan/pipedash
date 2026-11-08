import { Alert, Button, Center, Code, Group, List, Stack, Text, ThemeIcon } from '@mantine/core'
import { useInterval } from '@mantine/hooks'
import { IconAlertTriangle, IconCircleCheck } from '@tabler/icons-react'
import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react'

import { CopyButton } from '@/components/atoms/CopyButton'
import { ErrorState } from '@/components/common/ErrorState'
import { LoadingState } from '@/components/common/LoadingState'
import { StandardModal } from '@/components/common/StandardModal'
import { useCreatePairingCode } from '@/features/connection/queries/useConnectionQueries'
import { logger } from '@/lib/logger'
import { listenEvent } from '@/services/events'
import { WS_EVENTS } from '@/services/websocket'

interface AddDeviceModalProps {
  opened: boolean
  onClose: () => void
}

interface PairedDevice {
  id: string
  name: string
}

interface CopyRowProps {
  label: string
  value: string
  copyLabel: string
  showValue?: boolean
}

function CopyRow({ label, value, copyLabel, showValue = true }: CopyRowProps): ReactNode {
  return (
    <Group justify="space-between" wrap="nowrap" gap="sm">
      <Text size="sm" c="dimmed">
        {label}
      </Text>
      <Group gap="xs" wrap="nowrap" miw={0}>
        {showValue && <Code style={{ overflowWrap: 'anywhere' }}>{value}</Code>}
        <CopyButton value={value} label={copyLabel} />
      </Group>
    </Group>
  )
}

export function AddDeviceModal({ opened, onClose }: AddDeviceModalProps): ReactNode {
  const create = useCreatePairingCode()
  const { mutate } = create
  const requested = useRef(false)
  const [now, setNow] = useState(() => Date.now())
  const [paired, setPaired] = useState<PairedDevice | null>(null)
  const { start, stop } = useInterval(() => setNow(Date.now()), 1000)
  const pairing = create.data
  const code = pairing?.code

  const createCode = useCallback((): void => {
    mutate(undefined, { onSuccess: () => setNow(Date.now()) })
  }, [mutate])

  const close = (): void => {
    setPaired(null)
    onClose()
  }

  useEffect(() => {
    if (!opened) {
      requested.current = false

      return
    }

    if (!requested.current) {
      requested.current = true
      createCode()
    }
  }, [opened, createCode])

  useEffect(() => {
    if (!opened || !code) {
      return
    }

    start()

    return stop
  }, [opened, code, start, stop])

  useEffect(() => {
    if (!opened || !code) {
      return
    }

    let active = true
    let unlisten: (() => void) | undefined

    listenEvent<PairedDevice>(WS_EVENTS.DEVICE_PAIRED, setPaired)
      .then((stopListening) => {
        if (active) {
          unlisten = stopListening
        } else {
          stopListening()
        }
      })
      .catch((error: unknown) => logger.warn('AddDeviceModal', "Couldn't listen for paired devices", error))

    return () => {
      active = false
      unlisten?.()
    }
  }, [opened, code])

  let body: ReactNode
  if (paired) {
    body = (
      <Stack align="center" gap="sm" py="md">
        <ThemeIcon variant="transparent" color="green" size={48} aria-hidden>
          <IconCircleCheck size={48} />
        </ThemeIcon>
        <Text fw={600}>{paired.name} is connected.</Text>
        <Button onClick={close}>Done</Button>
      </Stack>
    )
  } else if (create.isError) {
    body = (
      <ErrorState
        title="Couldn't create a pairing code"
        error={create.error}
        onRetry={createCode}
        retrying={create.isPending}
      />
    )
  } else if (!pairing) {
    body = <LoadingState message="Creating pairing code" />
  } else {
    const remaining = Math.max(0, Math.ceil((Date.parse(pairing.expires_at) - now) / 1000))
    const countdown = `${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, '0')}`

    body = (
      <Stack gap="md">
        {pairing.loopback && (
          <Alert
            color="orange"
            title="Other devices can't reach this address"
            icon={<IconAlertTriangle size={16} aria-hidden />}
          >
            {pairing.server_url} only works on this computer. Open Pipedash with its network address, or enter that
            address on the new device.
          </Alert>
        )}

        <Center>
          <img
            src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(pairing.qr_svg)}`}
            width={240}
            height={240}
            alt="QR code that pairs a device with this Pipedash"
            style={{ background: 'var(--mantine-color-white)' }}
          />
        </Center>

        <List type="ordered" size="sm" spacing="xs">
          <List.Item>On the new device, open Pipedash and choose Connect to a server.</List.Item>
          <List.Item>Scan this code, or enter the address and code below.</List.Item>
        </List>

        <Stack gap="xs">
          <CopyRow label="Server address" value={pairing.server_url} copyLabel="Copy server address" />
          <CopyRow label="Pairing code" value={pairing.code} copyLabel="Copy pairing code" />
          <CopyRow label="Pairing link" value={pairing.link} copyLabel="Copy pairing link" showValue={false} />
        </Stack>

        {remaining > 0 ? (
          <Text size="sm" c="dimmed">
            Code expires in {countdown}
          </Text>
        ) : (
          <Group gap="sm">
            <Text size="sm" c="dimmed">
              This code expired.
            </Text>
            <Button size="xs" variant="default" onClick={createCode}>
              Create new code
            </Button>
          </Group>
        )}
      </Stack>
    )
  }

  return (
    <StandardModal opened={opened} onClose={close} title="Add device" size="md">
      {body}
    </StandardModal>
  )
}
