import { openConfirm } from '../../utils/confirm'

export function confirmDisconnect(host: string): Promise<boolean> {
  const { promise, resolve } = Promise.withResolvers<boolean>()

  openConfirm({
    title: `Disconnect from ${host}?`,
    children: `This device stops showing pipelines from ${host}. ${host} and its data stay as they are.`,
    labels: { confirm: 'Disconnect', cancel: 'Cancel' },
    confirmProps: { color: 'red' },
    onConfirm: () => resolve(true),
    onCancel: () => resolve(false),
    onClose: () => resolve(false),
  })

  return promise
}
