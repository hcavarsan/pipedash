import React, { Component, type ReactNode } from 'react'

interface FeatureErrorBoundaryProps {
  children: ReactNode
  featureName?: string
  fallback?: (error: Error, resetError: () => void) => ReactNode
}

interface FeatureErrorBoundaryState {
  hasError: boolean
  error: Error | null
}

export class FeatureErrorBoundary extends Component<
  FeatureErrorBoundaryProps,
  FeatureErrorBoundaryState
> {
  constructor(props: FeatureErrorBoundaryProps) {
    super(props)
    this.state = {
      hasError: false,
      error: null,
    }
  }

  static getDerivedStateFromError(error: Error): FeatureErrorBoundaryState {
    return {
      hasError: true,
      error,
    }
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo): void {
    const featureName = this.props.featureName || 'Unknown feature'

    console.error(`[FeatureErrorBoundary:${featureName}] Caught error:`, error, errorInfo)
  }

  resetError = (): void => {
    this.setState({
      hasError: false,
      error: null,
    })
  }

  render(): ReactNode {
    if (this.state.hasError && this.state.error) {
      if (this.props.fallback) {
        return this.props.fallback(this.state.error, this.resetError)
      }

      const featureName = this.props.featureName || 'Feature'

      return (
        <div style={{
          padding: '1rem',
          margin: '1rem 0',
          backgroundColor: '#25262b',
          borderRadius: '8px',
          border: '1px solid #fa5252',
          fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
        }}>
          <div style={{
            display: 'flex',
            alignItems: 'center',
            gap: '0.5rem',
            marginBottom: '0.75rem',
          }}>
            <span style={{ fontSize: '1.25rem' }}>⚠️</span>
            <span style={{
              fontWeight: 600,
              color: '#fa5252',
              fontSize: '0.875rem',
            }}>
              {featureName} encountered an error
            </span>
          </div>

          <p style={{
            margin: '0 0 1rem 0',
            fontSize: '0.8rem',
            color: '#909296',
          }}>
            {this.state.error.message || 'An unexpected error occurred'}
          </p>

          {import.meta.env.DEV && (
            <details style={{ marginBottom: '1rem' }}>
              <summary style={{
                cursor: 'pointer',
                fontSize: '0.75rem',
                color: '#909296',
                marginBottom: '0.5rem',
              }}>
                Show technical details
              </summary>
              <pre style={{
                padding: '0.75rem',
                backgroundColor: '#1a1b1e',
                borderRadius: '4px',
                fontSize: '0.65rem',
                overflow: 'auto',
                maxHeight: '150px',
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-word',
                margin: 0,
                color: '#c1c2c5',
              }}>
                {this.state.error.stack || this.state.error.message}
              </pre>
            </details>
          )}

          <button
            type="button"
            onClick={this.resetError}
            style={{
              padding: '0.4rem 0.75rem',
              backgroundColor: '#228be6',
              color: '#fff',
              border: 'none',
              borderRadius: '4px',
              fontSize: '0.75rem',
              cursor: 'pointer',
              fontWeight: 500,
            }}
          >
            Try again
          </button>
        </div>
      )
    }

    return this.props.children
  }
}
