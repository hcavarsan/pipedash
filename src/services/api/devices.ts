import type { PairedDevice, PairingCode } from '../../types/native'
import type { MobileDevicesResponse, PushRelaySettings } from '../../types/notifications'
import { apiServerUrl, type HttpClient } from '../http'
import type { DevicesService, PushService } from '../types'

export function createDevicesApi(client: HttpClient): DevicesService & PushService {
  return {
    listMobileDevices: async () => client.get<MobileDevicesResponse>('/push/subscriptions'),

    removeMobileDevice: async (id) => {
      await client.requestVoid('DELETE', `/push/subscriptions/${encodeURIComponent(id)}`)
    },

    testMobileDevice: async (id) => {
      await client.requestVoid('POST', `/push/subscriptions/${encodeURIComponent(id)}/test`)
    },

    getPushRelay: async () => client.get<PushRelaySettings>('/push/relay'),

    setPushRelay: async (enabled) => client.put<PushRelaySettings>('/push/relay', { enabled }),

    listPairedDevices: async () => (await client.get<{ devices: PairedDevice[] }>('/devices')).devices,

    createPairingCode: async () => client.post<PairingCode>('/devices/pairing', { server_url: apiServerUrl() }),

    removePairedDevice: async (id) => {
      await client.requestVoid('DELETE', `/devices/${encodeURIComponent(id)}`)
    },
  }
}
