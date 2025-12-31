import { RETRY_INTERVALS } from '../constants/intervals'
import type { RetryConfig } from '../types/utils'

export const DEFAULT_RETRY_CONFIG: RetryConfig = {
  maxAttempts: 5,
  delayMs: RETRY_INTERVALS.INITIAL,
  backoffMultiplier: 2,
  shouldRetry: (error: Error) => {
    const retryableErrors = [
      'NetworkError',
      'TimeoutError',
      'ECONNREFUSED',
      'ETIMEDOUT',
    ]

    return (
      retryableErrors.some((errType) => error.message.includes(errType)) ||
      (error.message.includes('status') &&
        /5\d{2}/.test(error.message))
    )
  },
}

export function shouldRetry(
  error: Error,
  attempt: number,
  config: RetryConfig = DEFAULT_RETRY_CONFIG
): boolean {
  if (attempt >= config.maxAttempts) {
    return false
  }

  if (config.shouldRetry) {
    return config.shouldRetry(error)
  }

  return true
}

export function getRetryDelay(
  attempt: number,
  config: RetryConfig = DEFAULT_RETRY_CONFIG
): number {
  const { delayMs, backoffMultiplier = 1 } = config

  return delayMs * backoffMultiplier ** (attempt - 1)
}

export async function withRetry<T>(
  fn: () => Promise<T>,
  config: RetryConfig = DEFAULT_RETRY_CONFIG
): Promise<T> {
  let lastError: Error | null = null
  let attempt = 1

  while (attempt <= config.maxAttempts) {
    try {
      return await fn()
    } catch (error) {
      lastError = error instanceof Error ? error : new Error(String(error))

      if (!shouldRetry(lastError, attempt, config)) {
        throw lastError
      }

      if (attempt < config.maxAttempts) {
        const delay = getRetryDelay(attempt, config)

        await sleep(delay)
      }

      attempt++
    }
  }

  throw lastError || new Error('All retry attempts failed')
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

