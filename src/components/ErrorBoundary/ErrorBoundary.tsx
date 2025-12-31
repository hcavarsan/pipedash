import { Component, ReactNode } from 'react'

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
  errorInfo: React.ErrorInfo | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props)
    this.state = { hasError: false, error: null, errorInfo: null }
  }

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('Error Boundary caught error:', error, errorInfo)
    this.setState({ errorInfo })
  }

  handleReload = () => {
    window.location.reload()
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null, errorInfo: null })
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback
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
              fontSize: '56px',
              marginBottom: '1.5rem',
            }}>
              ⚠️
            </div>
            <h2 style={{
              margin: '0 0 0.5rem 0',
              fontSize: '1.5rem',
              fontWeight: 600,
              color: '#fff',
            }}>
              Something went wrong
            </h2>
            <p style={{
              margin: '0 0 1.5rem 0',
              fontSize: '0.875rem',
              color: '#909296',
            }}>
              {this.state.error?.message || 'An unexpected error occurred'}
            </p>

            {import.meta.env.DEV && this.state.errorInfo && (
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
                  Show component stack
                </summary>
                <pre style={{
                  padding: '1rem',
                  backgroundColor: '#1a1b1e',
                  borderRadius: '4px',
                  fontSize: '0.65rem',
                  overflow: 'auto',
                  maxHeight: '200px',
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                  margin: 0,
                }}>
                  {this.state.errorInfo.componentStack}
                </pre>
              </details>
            )}

            <div style={{
              display: 'flex',
              flexDirection: 'column',
              gap: '0.5rem',
              maxWidth: '280px',
              margin: '0 auto',
            }}>
              <button
                type="button"
                onClick={this.handleReload}
                style={{
                  padding: '0.75rem 1rem',
                  backgroundColor: '#228be6',
                  color: '#fff',
                  border: 'none',
                  borderRadius: '4px',
                  fontSize: '0.875rem',
                  cursor: 'pointer',
                  fontWeight: 500,
                  width: '100%',
                }}
              >
                Reload Application
              </button>
              <button
                type="button"
                onClick={this.handleReset}
                style={{
                  padding: '0.75rem 1rem',
                  backgroundColor: 'transparent',
                  color: '#909296',
                  border: 'none',
                  borderRadius: '4px',
                  fontSize: '0.875rem',
                  cursor: 'pointer',
                  fontWeight: 500,
                  width: '100%',
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
