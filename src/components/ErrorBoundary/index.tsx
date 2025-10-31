import { Component, ReactNode } from 'react'

import { Button, Code, Container, Paper, Stack, Text, Title } from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'

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
        <Container size="sm" py="xl">
          <Paper p="xl" withBorder>
            <Stack align="center" gap="lg">
              <IconAlertTriangle size={56} color="var(--mantine-color-red-6)" />

              <Stack align="center" gap="xs">
                <Title order={2}>Something went wrong</Title>
                <Text c="dimmed" ta="center" size="sm">
                  {this.state.error?.message || 'An unexpected error occurred'}
                </Text>
              </Stack>

              {import.meta.env.DEV && this.state.errorInfo && (
                <Code block style={{ maxWidth: '100%', overflow: 'auto' }}>
                  {this.state.errorInfo.componentStack}
                </Code>
              )}

              <Stack gap="xs" maw={320} w="100%">
                <Button onClick={this.handleReload} fullWidth>
                  Reload Application
                </Button>
                <Button onClick={this.handleReset} variant="subtle" fullWidth>
                  Try Again
                </Button>
              </Stack>
            </Stack>
          </Paper>
        </Container>
      )
    }

    return this.props.children
  }
}
