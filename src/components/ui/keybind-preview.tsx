import { KeybindConfig } from '@/lib/use-keybinds'
import { cn } from '@/lib/utils'

type KeybindPreviewProps = {
  keybind: KeybindConfig
  className?: string
}

export function KeybindPreview({ keybind, className }: KeybindPreviewProps) {
  const hasCtrl = keybind.keyMatcher.toString().includes('ctrl')
  const hasShift = keybind.keyMatcher.toString().includes('shift')
  const hasMeta = keybind.keyMatcher.toString().includes('meta')
  const hasAlt = keybind.keyMatcher.toString().includes('alt')
  const key = /event\.key === ['"](\w+)['"]/.exec(keybind.keyMatcher.toString())?.[1]

  if (!key) return null

  return (
    <div className={cn('flex items-center gap-1 text-xs text-muted-foreground font-mono', className)}>
      {hasCtrl && (
        <span className='rounded-sm bg-muted text-foreground w-6 h-6 flex items-center justify-center'>⌘</span>
      )}
      {hasShift && (
        <span className='rounded-sm bg-muted text-foreground w-6 h-6 flex items-center justify-center'>⇧</span>
      )}
      {hasMeta && (
        <span className='rounded-sm bg-muted text-foreground w-6 h-6 flex items-center justify-center'>⌘</span>
      )}
      {hasAlt && (
        <span className='rounded-sm bg-muted text-foreground w-6 h-6 flex items-center justify-center'>⌥</span>
      )}
      <span className='rounded-sm bg-muted text-foreground w-6 h-6 flex items-center justify-center'>
        {key?.toUpperCase()}
      </span>
    </div>
  )
}
