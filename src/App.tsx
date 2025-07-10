import './App.css'
import { Button } from '@/components/ui/button'
import { useKeybinds } from '@/lib/use-keybinds'
import { useState } from 'react'

function App() {
  const [focusedButtonIndex, setFocusedButtonIndex] = useState(0)
  useKeybinds([
    {
      keyMatcher: (event: KeyboardEvent) => event.key === 'ArrowDown',
      callback: () => setFocusedButtonIndex((prev) => (prev + 1) % 3),
    },
    {
      keyMatcher: (event: KeyboardEvent) => event.key === 'ArrowUp',
      callback: () => setFocusedButtonIndex((prev) => (prev - 1 + 3) % 3),
    },
  ])

  return (
    <div className='min-h-screen flex items-center justify-center bg-background'>
      <div className='text-center'>
        <div className='flex flex-col gap-2 w-50'>
          <Button>Button 1 {focusedButtonIndex === 0 ? 'focused' : ''}</Button>
          <Button>Button 2 {focusedButtonIndex === 1 ? 'focused' : ''}</Button>
          <Button>Button 3 {focusedButtonIndex === 2 ? 'focused' : ''}</Button>
        </div>
      </div>
    </div>
  )
}

export default App
