import {
  Button,
  Card,
  Divider,
  Input,
  type MantineColorScheme,
  SegmentedControl,
  Select,
  Stack,
  Text,
  Title,
  useMantineColorScheme,
} from '@mantine/core'
import { IconTrash } from '@tabler/icons-react'
import { useEffect, useState } from 'react'

import { getPlatformOverride, platform, setPlatformOverride } from '@/utils/platform'

import { useFactoryResetConfirm } from '../useFactoryResetConfirm'

interface GeneralSectionProps {
  onRefresh?: () => Promise<void>
}

const THEME_OPTIONS = [
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Light' },
  { value: 'auto', label: 'System' },
]

export function GeneralSection({ onRefresh }: GeneralSectionProps) {
  const [platformOverride, setPlatformOverrideState] = useState<string | null>(null)
  const [currentPlatform, setCurrentPlatform] = useState<string>('linux')
  const { colorScheme, setColorScheme } = useMantineColorScheme()
  const { confirmReset, isResetting } = useFactoryResetConfirm(async () => {
    setPlatformOverrideState(null)
    await onRefresh?.()
  })

  useEffect(() => {
    const loadPlatform = async () => {
      const override = getPlatformOverride()

      setPlatformOverrideState(override)
      const detectedPlatform = await platform()

      setCurrentPlatform(detectedPlatform)
    }

    loadPlatform()
  }, [])

  const handlePlatformChange = (value: string | null) => {
    setPlatformOverride(value as 'macos' | 'windows' | 'linux' | null)
    setPlatformOverrideState(value)
  }

  return (
    <>
      <Title order={2} size="lg" fw={600} mb="lg">
        General
      </Title>

      <Stack gap="md">
        <Card padding="md">
          <Stack gap="md">
            <Input.Wrapper label="Theme" description="Choose Dark, Light, or follow your system setting.">
              <SegmentedControl
                mt="xs"
                fullWidth
                data={THEME_OPTIONS}
                value={colorScheme}
                onChange={(value) => setColorScheme(value as MantineColorScheme)}
              />
            </Input.Wrapper>

            <Divider />

            <Select
              label="Platform"
              description={`Detected: ${currentPlatform}`}
              placeholder="Auto-detect"
              data={[
                { value: '', label: 'Auto-detect' },
                { value: 'macos', label: 'macOS' },
                { value: 'windows', label: 'Windows' },
                { value: 'linux', label: 'Linux' },
              ]}
              value={platformOverride || ''}
              onChange={handlePlatformChange}
            />
          </Stack>
        </Card>

        <Card padding="md">
          <Stack gap="md">
            <Stack gap={0}>
              <Text size="sm" fw={600} c="red">
                Danger zone
              </Text>
              <Text size="xs" c="dimmed">
                Permanently delete all application data
              </Text>
            </Stack>

            <Divider />

            <Button
              leftSection={<IconTrash size={16} />}
              variant="light"
              color="red"
              fullWidth
              onClick={confirmReset}
              loading={isResetting}
            >
              Factory reset
            </Button>
          </Stack>
        </Card>
      </Stack>
    </>
  )
}
