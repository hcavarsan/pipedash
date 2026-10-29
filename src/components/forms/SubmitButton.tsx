import { Button, type ButtonProps } from '@mantine/core'
import type { ReactNode } from 'react'
import { useFormStatus } from 'react-dom'

type SubmitButtonProps = Omit<ButtonProps, 'loading'> & { children: ReactNode; disabled?: boolean | undefined }

export function SubmitButton({ disabled, children, ...props }: SubmitButtonProps): ReactNode {
  const { pending } = useFormStatus()

  return (
    <Button {...props} type="submit" loading={pending} disabled={disabled === true || pending}>
      {children}
    </Button>
  )
}
