export interface RetryConfig {
  maxAttempts: number
  delayMs: number
  backoffMultiplier?: number
  shouldRetry?: (error: Error) => boolean
}
