import React, { Component, type ReactNode } from 'react'

interface RouteErrorBoundaryProps {
  children: ReactNode
  fallback?: (error: Error, resetError: () => void) => ReactNode
}

interface RouteErrorBoundaryState {
  hasError: boolean
  error: Error | null
}

export class RouteErrorBoundary extends Component<
  RouteErrorBoundaryProps,
  RouteErrorBoundaryState
> {
  constructor(props: RouteErrorBoundaryProps) {
    super(props)
    this.state = {
      hasError: false,
      error: null,
    }
  }

  static getDerivedStateFromError(error: Error): RouteErrorBoundaryState {
    return {
      hasError: true,
      error,
    }
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo): void {
    console.error('[RouteErrorBoundary] Caught error:', error, errorInfo)
  }

  resetError = (): void => {
    this.setState({
      hasError: false,
      error: null,
    })
  }

  handleReload = (): void => {
    window.location.reload()
  }

  render(): ReactNode {
    if (this.state.hasError && this.state.error) {
      if (this.props.fallback) {
        return this.props.fallback(this.state.error, this.resetError)
      }

      return (
        <div style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          minHeight: '100vh',
          padding: '2rem',
          backgroundColor: '#1a1b1e',
          color: '#c1c2c5',
          fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
        }}>
          <div style={{
            maxWidth: '500px',
            width: '100%',
            padding: '2rem',
            backgroundColor: '#25262b',
            borderRadius: '8px',
            border: '1px solid #373a40',
            textAlign: 'center',
          }}>
            <div style={{
              fontSize: '48px',
              marginBottom: '1rem',
            }}>
              ⚠️
            </div>
            <h2 style={{
              margin: '0 0 0.5rem 0',
              fontSize: '1.25rem',
              fontWeight: 600,
              color: '#fff',
            }}>
              Page failed to load
            </h2>
            <p style={{
              margin: '0 0 1.5rem 0',
              fontSize: '0.875rem',
              color: '#909296',
            }}>
              {this.state.error.message || 'An unexpected error occurred'}
            </p>

            {import.meta.env.DEV && (
              <details style={{
                marginBottom: '1.5rem',
                textAlign: 'left',
              }}>
                <summary style={{
                  cursor: 'pointer',
                  fontSize: '0.75rem',
                  color: '#909296',
                  marginBottom: '0.5rem',
                }}>
                  Show technical details
                </summary>
                <pre style={{
                  padding: '1rem',
                  backgroundColor: '#1a1b1e',
                  borderRadius: '4px',
                  fontSize: '0.7rem',
                  overflow: 'auto',
                  maxHeight: '200px',
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                  margin: 0,
                }}>
                  {this.state.error.stack || this.state.error.message}
                </pre>
              </details>
            )}

            <div style={{ display: 'flex', gap: '0.5rem', justifyContent: 'center' }}>
              <button
                type="button"
                onClick={this.handleReload}
                style={{
                  padding: '0.5rem 1rem',
                  backgroundColor: '#228be6',
                  color: '#fff',
                  border: 'none',
                  borderRadius: '4px',
                  fontSize: '0.875rem',
                  cursor: 'pointer',
                  fontWeight: 500,
                }}
              >
                Reload Page
              </button>
              <button
                type="button"
                onClick={this.resetError}
                style={{
                  padding: '0.5rem 1rem',
                  backgroundColor: 'transparent',
                  color: '#909296',
                  border: '1px solid #373a40',
                  borderRadius: '4px',
                  fontSize: '0.875rem',
                  cursor: 'pointer',
                  fontWeight: 500,
                }}
              >
                Try Again
              </button>
            </div>
          </div>
        </div>
      )
    }

    return this.props.children
  }
}
