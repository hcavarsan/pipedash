import { Badge, Box, Center, Group, Loader, Paper, Stack, Text, Title, UnstyledButton } from '@mantine/core'
import { IconDeviceDesktop, IconServer2 } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { useNavigate } from 'react-router-dom'

import { AppLogo } from '../../../components/atoms/AppLogo'
import { DocumentTitle } from '../../../components/common/DocumentTitle'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { withBasePath } from '../../../lib/basePath'
import { isMobilePlatform } from '../../../utils/platform'
import { Onboarding } from '../../setup/components/SetupWizard/Onboarding'
import { useRunOnThisDevice } from '../queries/useConnectionQueries'

const TITLE = 'Where should Pipedash run?'
const FOOTER = 'You can change this later in Settings → Devices.'
const LOCAL_TITLE = 'On this device'
const SERVER_TITLE = 'Connect to a server'
const SERVER_TEXT = 'Use Pipedash running on a server or on another computer. This device only shows its pipelines.'

const RECOMMENDED = (
  <Badge variant="light" color="blue">
    Recommended
  </Badge>
)

interface DesktopChoiceProps {
  icon: ReactNode
  title: string
  text: string
  recommended: boolean
  busy?: boolean
  disabled?: boolean
  onClick: () => void
}

function DesktopChoice({ icon, title, text, recommended, busy, disabled, onClick }: DesktopChoiceProps): ReactNode {
  return (
    <UnstyledButton
      className="interactive-surface"
      p="md"
      bdrs="sm"
      bd="1px solid var(--mantine-color-default-border)"
      w="100%"
      aria-busy={busy || undefined}
      aria-disabled={disabled || undefined}
      onClick={disabled ? undefined : onClick}
    >
      <Group gap="md" wrap="nowrap" align="flex-start">
        {icon}
        <Stack gap="2xs" flex={1} miw={0}>
          <Group gap="xs">
            <Text fw={600}>{title}</Text>
            {recommended && RECOMMENDED}
          </Group>
          <Text size="sm" c="dimmed">
            {text}
          </Text>
        </Stack>
        {busy && <Loader size="sm" />}
      </Group>
    </UnstyledButton>
  )
}

export function WelcomeScreen(): ReactNode {
  const navigate = useNavigate()
  const mobileShell = useMobileShell()
  const runLocal = useRunOnThisDevice()
  const recommendLocal = !isMobilePlatform()
  const localText = mobileShell
    ? 'Add your providers here. This phone checks them itself.'
    : 'Add your providers here. Later you can share this computer with your phone.'

  const chooseLocal = (): void => {
    runLocal.mutate(undefined, { onSuccess: () => window.location.replace(withBasePath('/')) })
  }
  const chooseServer = (): void => {
    void navigate('/connect')
  }

  if (mobileShell) {
    return (
      <Onboarding title={TITLE}>
        <DocumentTitle title="Welcome" />
        <IosGroup footer={FOOTER}>
          <IosRow
            title={LOCAL_TITLE}
            subtitle={localText}
            icon={<IconDeviceDesktop size={18} />}
            accessory={runLocal.isPending ? <Loader size="sm" /> : recommendLocal ? RECOMMENDED : undefined}
            chevron={!runLocal.isPending}
            disabled={runLocal.isPending}
            onClick={chooseLocal}
          />
          <IosRow
            title={SERVER_TITLE}
            subtitle={SERVER_TEXT}
            icon={<IconServer2 size={18} />}
            accessory={recommendLocal ? undefined : RECOMMENDED}
            chevron
            disabled={runLocal.isPending}
            onClick={chooseServer}
          />
        </IosGroup>
      </Onboarding>
    )
  }

  return (
    <Box component="main" h="100vh" className="safe-area">
      <DocumentTitle title="Welcome" />
      <Center h="100%" p={{ base: 'md', sm: 'xl' }}>
        <Paper p={{ base: 'lg', sm: 'xl' }} w="100%" maw={480} withBorder>
          <Stack gap="xl">
            <Stack gap="md" align="center">
              <AppLogo size={{ base: 48, sm: 56 }} alt="Pipedash" />
              <Title order={1} size="h2" ta="center">
                {TITLE}
              </Title>
            </Stack>

            <Stack gap="sm">
              <DesktopChoice
                icon={<IconDeviceDesktop size={24} aria-hidden />}
                title={LOCAL_TITLE}
                text={localText}
                recommended={recommendLocal}
                busy={runLocal.isPending}
                disabled={runLocal.isPending}
                onClick={chooseLocal}
              />
              <DesktopChoice
                icon={<IconServer2 size={24} aria-hidden />}
                title={SERVER_TITLE}
                text={SERVER_TEXT}
                recommended={!recommendLocal}
                disabled={runLocal.isPending}
                onClick={chooseServer}
              />
            </Stack>

            <Text size="sm" c="dimmed" ta="center">
              {FOOTER}
            </Text>
          </Stack>
        </Paper>
      </Center>
    </Box>
  )
}
