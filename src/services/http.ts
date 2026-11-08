import { invoke } from '@tauri-apps/api/core'

import { API_TIMEOUTS } from '../constants/timeouts'
import { withBasePath } from '../lib/basePath'
import { logger } from '../lib/logger'
import { getToken, useAuthStore } from '../stores/authStore'
import { formatErrorMessage, getErrorCode, getHttpStatus } from '../types/errors'
import type { RemoteResponse } from '../types/native'
import { DEFAULT_RETRY_CONFIG, withRetry } from '../utils/retryLogic'

import { getConnection, isRemoteClient } from './connection'

export const API_BASE = import.meta.env.VITE_API_URL || withBasePath('/api/v1')

export function apiServerUrl(): string {
  if (isRemoteClient()) {
    return getConnection().serverUrl ?? ''
  }

  return new URL(API_BASE, window.location.href).href.replace(/\/api\/v1\/?$/, '')
}

export interface RequestOptions {
  timeoutMs?: number | undefined
  signal?: AbortSignal | undefined
}

const parseErrorBody = (body: string): { code?: string | undefined; message?: string | undefined } => {
  try {
    const parsed: unknown = JSON.parse(body)
    const message =
      typeof parsed === 'object' && parsed !== null && 'message' in parsed && typeof parsed.message === 'string'
        ? parsed.message
        : undefined

    return { code: getErrorCode(parsed), message }
  } catch {
    return {}
  }
}

const OPERATION_TIMEOUTS: Record<string, number> = {
  '/factory-reset': API_TIMEOUTS.EXTENDED,
  '/pipelines': API_TIMEOUTS.LONG,
  '/providers/preview': API_TIMEOUTS.UPLOAD,
  '/providers/organizations': API_TIMEOUTS.UPLOAD,
  default: API_TIMEOUTS.DEFAULT,
}

const getTimeoutFor = (method: string, path: string): number => {
  if (method === 'POST' && path === '/providers/draft') {
    return API_TIMEOUTS.EXTENDED
  }

  for (const [key, timeout] of Object.entries(OPERATION_TIMEOUTS)) {
    if (path.includes(key)) {
      return timeout
    }
  }

  return OPERATION_TIMEOUTS.default ?? API_TIMEOUTS.DEFAULT
}

export class HttpClient {
  private async fetchWithTimeout(
    url: string,
    options: RequestInit,
    timeoutMs: number = API_TIMEOUTS.DEFAULT,
    signal?: AbortSignal
  ): Promise<Response> {
    if (signal?.aborted) {
      throw signal.reason
    }

    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), timeoutMs)
    const onAbort = () => controller.abort(signal?.reason)

    signal?.addEventListener('abort', onAbort, { once: true })

    try {
      return await fetch(url, {
        ...options,
        signal: controller.signal,
      })
    } catch (error) {
      if (!signal?.aborted && error instanceof Error && error.name === 'AbortError') {
        throw new Error(`Request timeout after ${timeoutMs}ms`)
      }
      throw error
    } finally {
      clearTimeout(timeoutId)
      signal?.removeEventListener('abort', onAbort)
    }
  }

  private async remoteFetch(
    method: string,
    path: string,
    body: string | undefined,
    timeoutMs: number,
    signal?: AbortSignal
  ): Promise<Response> {
    if (signal?.aborted) {
      throw signal.reason
    }

    let res: RemoteResponse

    try {
      res = await invoke<RemoteResponse>('remote_request', { method, path, body, timeoutMs })
    } catch (error) {
      if (getErrorCode(error) === 'TIMEOUT') {
        throw new Error(`Request timeout after ${timeoutMs}ms`)
      }
      throw new Error(formatErrorMessage(error))
    }

    if (signal?.aborted) {
      throw signal.reason
    }

    return new Response(res.body || null, { status: res.status })
  }

  private async send(method: string, path: string, body?: unknown, options: RequestOptions = {}): Promise<string> {
    return withRetry(
      async () => {
        const token = getToken()
        const headers: Record<string, string> = {
          'Content-Type': 'application/json',
        }

        if (token) {
          headers.Authorization = `Bearer ${token}`
        }
        const payload = body ? JSON.stringify(body) : undefined
        const timeoutMs = options.timeoutMs ?? getTimeoutFor(method, path)
        const init: RequestInit = { method, headers, ...(payload ? { body: payload } : {}) }
        const res = isRemoteClient()
          ? await this.remoteFetch(method, path, payload, timeoutMs, options.signal)
          : await this.fetchWithTimeout(`${API_BASE}${path}`, init, timeoutMs, options.signal)

        if (res.ok) {
          useAuthStore.getState().resetFailures()
        }

        if (!res.ok) {
          if (res.status === 401 && !isRemoteClient()) {
            useAuthStore.getState().incrementFailure()
          }
          const errorText = await res.text()
          const { code, message } = parseErrorBody(errorText)
          const error = Object.assign(new Error(message || errorText || `HTTP ${res.status}`), {
            status: res.status,
            code,
          })

          throw error
        }

        return res.text()
      },
      {
        ...DEFAULT_RETRY_CONFIG,
        maxAttempts: method === 'GET' ? DEFAULT_RETRY_CONFIG.maxAttempts : 1,
        shouldRetry: (error: Error) => {
          const status = getHttpStatus(error) ?? 0
          const isNetworkError = error instanceof TypeError
          const isTimeout = error.message.includes('timeout')
          const is5xxError = status >= 500 && status < 600
          const isRateLimit = status === 429

          return isNetworkError || isTimeout || is5xxError || isRateLimit
        },
      }
    )
  }

  async request<T>(method: string, path: string, body?: unknown, options: RequestOptions = {}): Promise<T> {
    const text = await this.send(method, path, body, options)

    if (!text) {
      throw new Error('Empty response from server')
    }

    try {
      return JSON.parse(text)
    } catch (parseError) {
      logger.error('API', 'Failed to parse response', parseError)
      throw new Error('Invalid response from server: response is not valid JSON')
    }
  }

  async requestVoid(method: string, path: string, body?: unknown, options: RequestOptions = {}): Promise<void> {
    await this.send(method, path, body, options)
  }

  get<T>(path: string): Promise<T> {
    return this.request<T>('GET', path)
  }

  post<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>('POST', path, body, options)
  }

  put<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('PUT', path, body)
  }

  delete<T>(path: string): Promise<T> {
    return this.request<T>('DELETE', path)
  }
}
