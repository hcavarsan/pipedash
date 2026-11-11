import { invoke } from '@tauri-apps/api/core'

import type { PipedashError } from '../types/errors'
import { toPipedashError } from '../types/errors'

const TAURI_COMMAND_TIMEOUTS: Record<string, number> = {
  factory_reset: 120000,
  list_pipelines: 90000,
  get_cached_pipelines: 90000,
  preview_provider_pipelines: 60000,
  fetch_provider_organizations: 60000,
  save_provider_draft: 120000,
}

const DEFAULT_COMMAND_TIMEOUT = 45000

function timeoutError(cmd: string, timeoutMs: number): PipedashError {
  return { type: 'timeout', message: `Command '${cmd}' timeout`, timeoutMs }
}

export async function invokeWithTimeout<T>(
  cmd: string,
  args?: Record<string, unknown>,
  customTimeout?: number
): Promise<T> {
  const timeout = customTimeout ?? TAURI_COMMAND_TIMEOUTS[cmd] ?? DEFAULT_COMMAND_TIMEOUT
  const timeoutGate = Promise.withResolvers<T>()
  const timer = setTimeout(() => timeoutGate.reject(timeoutError(cmd, timeout)), timeout)

  try {
    return await Promise.race([invoke<T>(cmd, args), timeoutGate.promise])
  } catch (error) {
    throw toPipedashError(error)
  } finally {
    clearTimeout(timer)
  }
}
