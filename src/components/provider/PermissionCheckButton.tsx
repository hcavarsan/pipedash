import { Box, Button } from '@mantine/core'
import { IconShieldCheck } from '@tabler/icons-react'

interface PermissionCheckButtonProps {
  onClick: () => void
  disabled?: boolean
  loading?: boolean
}

export const PermissionCheckButton = ({
  onClick,
  disabled = false,
  loading = false,
}: PermissionCheckButtonProps) => {
  return (
    <Button
      variant="light"
      color={disabled ? 'gray' : 'blue'}
      size="sm"
      onClick={onClick}
      disabled={disabled}
      loading={loading}
      leftSection={
        <Box component="span" c="blue">
          <IconShieldCheck size={16} />
        </Box>
      }
    >
      Check Permissions
    </Button>
  )
}
