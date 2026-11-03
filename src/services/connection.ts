import type { SharingStatus } from '../types/domain'

import { isTauri } from './isTauri'
import { call } from './mobile'

export const REMOTE_ACCESS_REVOKED_EVENT = 'remote-access-revoked'

export type ConnectionMode = 'unset' | 'standalone' | 'client'
export type HostKind = 'server' | 'desktop'
export type RemoteStatus = 'connecting' | 'connected' | 'reconnecting' | 'removed'

export interface ConnectionInfo {
  mode: ConnectionMode
  serverUrl?: string
  host?: string
  hostKind?: HostKind
  deviceId?: string
  status?: RemoteStatus
  allowPrivateHttp?: boolean
}

let current: ConnectionInfo = { mode: 'standalone' }

export async function loadConnection(): Promise<void> {
  if (isTauri()) {
    current = await call<ConnectionInfo>('get_connection')
  }
}

export const getConnection = (): ConnectionInfo => current

export const isRemoteClient = (): boolean => isTauri() && current.mode === 'client'

export const hasLocalCore = (): boolean => isTauri() && current.mode === 'standalone'

export const connectionService = {
  pair: (address: string, code: string | null, allowPrivateHttp: boolean): Promise<ConnectionInfo> =>
    call('pair_with_server', { address, code, allowPrivateHttp }),

  runOnThisDevice: (): Promise<ConnectionInfo> => call('run_on_this_device'),

  disconnect: (): Promise<ConnectionInfo> => call('disconnect_server'),

  reconnect: (): Promise<void> => call('reconnect_server'),

  setAllowPrivateHttp: async (enabled: boolean): Promise<void> => {
    await call<void>('set_allow_private_http', { enabled })
    current = { ...current, allowPrivateHttp: enabled }
  },
}

export const sharingService = {
  getStatus: (): Promise<SharingStatus> => call('get_sharing_status'),

  setEnabled: (enabled: boolean): Promise<SharingStatus> => call('set_sharing', { enabled }),

  resetPort: (): Promise<SharingStatus> => call('reset_sharing_port'),
}
