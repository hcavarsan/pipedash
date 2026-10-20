import { Button } from '@mantine/core'
import { IconShieldCheck } from '@tabler/icons-react'

interface PermissionCheckButtonProps {
  onClick: () => void
  disabled?: boolean
  loading?: boolean
}

export const PermissionCheckButton = ({ onClick, disabled = false, loading = false }: PermissionCheckButtonProps) => {
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
