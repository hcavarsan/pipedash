import { isRecord } from '../utils/isRecord'

export type PipedashError =
  | { type: 'network'; message: string; statusCode?: number; cause?: unknown }
  | { type: 'auth'; message: string; cause?: unknown }
  | { type: 'validation'; message: string; fields: Record<string, string> }
  | {
      type: 'permission'
      message: string
      missingPermissions: string[]
      cause?: unknown
    }
  | { type: 'timeout'; message: string; timeoutMs?: number; cause?: unknown }
  | { type: 'not_found'; message: string; resource?: string; cause?: unknown }
  | { type: 'unknown'; message: string; cause?: unknown }

function optional(value: unknown, type: 'string' | 'number'): boolean {
  return value === undefined || typeof value === type
}

export function isPipedashError(error: unknown): error is PipedashError {
  if (!isRecord(error) || typeof error.message !== 'string') {
    return false
  }

  switch (error.type) {
    case 'network':
      return optional(error.statusCode, 'number')
    case 'timeout':
      return optional(error.timeoutMs, 'number')
    case 'not_found':
      return optional(error.resource, 'string')
    case 'auth':
    case 'unknown':
      return true
    case 'validation':
      return isRecord(error.fields)
    case 'permission':
      return Array.isArray(error.missingPermissions)
    default:
      return false
  }
}

export function formatErrorMessage(error: unknown): string {
  if (isPipedashError(error)) {
    return error.message
  }

  if (error instanceof Error) {
    return error.message
  }

  if (typeof error === 'string') {
    return error
  }

  if (isCommandError(error)) {
    return error.error
  }

  return 'An unknown error occurred'
}

export function getHttpStatus(error: unknown): number | undefined {
  if (error instanceof Error && 'status' in error && typeof error.status === 'number') {
    return error.status
  }

  return undefined
}

export function getErrorCode(error: unknown): string | undefined {
  if (typeof error !== 'object' || error === null) {
    return undefined
  }

  if ('code' in error && typeof error.code === 'string') {
    return error.code
  }

  if ('cause' in error && typeof error.cause === 'object' && error.cause !== null) {
    return getErrorCode(error.cause)
  }

  return undefined
}

function statusOf(error: unknown): number | undefined {
  const status = getHttpStatus(error)

  if (status !== undefined || !isRecord(error)) {
    return status
  }

  return getHttpStatus(error.cause)
}

function classifyByCode(error: unknown): PipedashError | undefined {
  const code = getErrorCode(error)
  const status = statusOf(error)
  const message = formatErrorMessage(error)

  switch (code) {
    case 'UNAUTHORIZED':
    case 'PROVIDER_AUTH_FAILED':
      return { type: 'auth', message, cause: error }
    case 'FORBIDDEN':
      return { type: 'permission', message, missingPermissions: [], cause: error }
    case 'NOT_FOUND':
      return { type: 'not_found', message, cause: error }
    case 'TIMEOUT':
      return { type: 'timeout', message, cause: error }
    case 'NETWORK_ERROR':
    case 'SERVICE_UNAVAILABLE':
    case 'SERVER_NOT_READY':
    case 'UNREACHABLE':
      return { type: 'network', message, ...(status !== undefined ? { statusCode: status } : {}), cause: error }
    case undefined:
      return status === 502 || status === 503 || status === 504
        ? { type: 'network', message, statusCode: status, cause: error }
        : undefined
    default:
      return undefined
  }
}

export function toPipedashError(error: unknown): PipedashError {
  if (isPipedashError(error) && error.type !== 'unknown') {
    return error
  }

  const classified = classifyByCode(error)

  if (classified) {
    return classified
  }

  if (error instanceof Error) {
    if (error.name === 'TypeError' && error.message.includes('fetch')) {
      return { type: 'network', message: 'Network request failed', cause: error }
    }

    if (error.name === 'TimeoutError' || error.message.includes('timeout')) {
      return { type: 'timeout', message: 'Request timed out', cause: error }
    }

    return { type: 'unknown', message: error.message, cause: error }
  }

  if (isPipedashError(error)) {
    return error
  }

  if (typeof error === 'string') {
    return { type: 'unknown', message: error }
  }

  if (isCommandError(error)) {
    return { type: 'unknown', message: error.error, cause: error }
  }

  return { type: 'unknown', message: 'An unknown error occurred', cause: error }
}

export function isTransientError(error: unknown): boolean {
  const { type } = toPipedashError(error)

  return type === 'network' || type === 'timeout'
}

function isCommandError(error: unknown): error is { error: string } {
  return typeof error === 'object' && error !== null && 'error' in error && typeof error.error === 'string'
}

export function isNetworkError(error: PipedashError): error is Extract<PipedashError, { type: 'network' }> {
  return error.type === 'network'
}

export function isAuthError(error: PipedashError): error is Extract<PipedashError, { type: 'auth' }> {
  return error.type === 'auth'
}

export function isValidationError(error: PipedashError): error is Extract<PipedashError, { type: 'validation' }> {
  return error.type === 'validation'
}

export function isPermissionError(error: PipedashError): error is Extract<PipedashError, { type: 'permission' }> {
  return error.type === 'permission'
}
