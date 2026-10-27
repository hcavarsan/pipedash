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

export function createError<T extends PipedashError['type']>(
  type: T,
  message: string,
  extra?: Partial<Extract<PipedashError, { type: T }>>
): PipedashError {
  return { type, message, ...extra } as PipedashError
}

export function isPipedashError(error: unknown): error is PipedashError {
  return (
    typeof error === 'object' &&
    error !== null &&
    'type' in error &&
    'message' in error &&
    typeof (error as { type: unknown }).type === 'string' &&
    typeof (error as { message: unknown }).message === 'string'
  )
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

export function toPipedashError(error: unknown): PipedashError {
  if (isPipedashError(error)) {
    return error
  }

  if (error instanceof Error) {
    if (error.name === 'TypeError' && error.message.includes('fetch')) {
      return createError('network', 'Network request failed', { cause: error })
    }

    if (error.name === 'TimeoutError' || error.message.includes('timeout')) {
      return createError('timeout', 'Request timed out', { cause: error })
    }

    return createError('unknown', error.message, { cause: error })
  }

  if (typeof error === 'string') {
    return createError('unknown', error)
  }

  if (isCommandError(error)) {
    return createError('unknown', error.error, { cause: error })
  }

  return createError('unknown', 'An unknown error occurred', { cause: error })
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
