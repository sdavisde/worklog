import { KeybindConfig } from '@/lib/use-keybinds'
import { Button, ButtonProps } from './button'
import { KeybindPreview } from './keybind-preview'
import { cn } from '@/lib/utils'
import { Link } from 'react-router'

type ActionButtonProps = ButtonProps & {
  to: string
  icon: React.ReactNode
  keybind?: KeybindConfig
}

export function ActionButton({ children, icon, keybind, to, ...props }: ActionButtonProps) {
  return (
    <Link
      to={to}
      className='w-full h-full'
    >
      <Button
        {...props}
        className={cn('flex items-center justify-start gap-2', props.className)}
      >
        <span className='size-4'>{icon}</span>
        {children}
        {keybind && (
          <KeybindPreview
            keybind={keybind}
            className='ml-auto'
          />
        )}
      </Button>
    </Link>
  )
}
