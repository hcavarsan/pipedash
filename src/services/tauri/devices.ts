import { invoke } from '@tauri-apps/api/core'

import type { PairedDevice, PairingCode } from '../../types/native'
import type { PushRelaySettings } from '../../types/notifications'
import type { DevicesService } from '../types'

export const devicesTauriService: DevicesService = {
  listPairedDevices: async () => invoke<PairedDevice[]>('list_paired_devices'),

  createPairingCode: async () => invoke<PairingCode>('create_pairing_code'),

  removePairedDevice: async (id) => invoke<void>('remove_paired_device', { id }),

  getPushRelay: async () => invoke<PushRelaySettings>('get_push_relay'),

  setPushRelay: async (enabled) => invoke<PushRelaySettings>('set_push_relay', { enabled }),
}
