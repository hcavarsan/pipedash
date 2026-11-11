import { Text } from '@mantine/core'
import { Z } from '../../../../theme/zIndex'
import { openConfirm } from '../../../../utils/confirm'

export function confirmSkipTransfer(onConfirm: () => void): void {
  openConfirm({
    title: 'Skip data transfer?',
    centered: true,
    zIndex: Z.alert,
    children: (
      <Text size="sm">
        You are changing the storage <strong>without transferring data</strong>. Existing providers, credentials and
        history stay in the previous location and won't be available after switching.
      </Text>
    ),
    labels: {
      confirm: 'Skip data transfer',
      cancel: 'Go back',
    },
    confirmProps: { color: 'red' },
    onConfirm,
  })
}
