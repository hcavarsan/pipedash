import { Button, Loader } from '@mantine/core'
import { IconShieldCheck } from '@tabler/icons-react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { IosGroup, IosRow } from '../ios/IosList'

interface PermissionCheckButtonProps {
  onClick: () => void
  disabled?: boolean
  loading?: boolean
}

export const PermissionCheckButton = ({ onClick, disabled = false, loading = false }: PermissionCheckButtonProps) => {
  const mobileShell = useMobileShell()

  if (mobileShell) {
    return (
      <IosGroup footer="Verify that these credentials grant the access Pipedash needs.">
        <IosRow
          icon={<IconShieldCheck size={18} />}
          title="Check permissions"
          chevron={!loading}
          accessory={loading ? <Loader size="sm" aria-label="Checking permissions" /> : undefined}
          onClick={onClick}
          disabled={disabled}
        />
      </IosGroup>
    )
  }

  return (
    <Button
      variant="default"
      size="sm"
      onClick={onClick}
      disabled={disabled}
      loading={loading}
      leftSection={<IconShieldCheck size={16} />}
    >
      Check permissions
    </Button>
  )
}
