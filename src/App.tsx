import './App.css'
import { useKeybinds } from '@/lib/use-keybinds'
import { useEffect, useState } from 'react'
import { ActionButton } from './components/ui/action-button'
import { Eye, Pencil, NotebookPen, PenIcon } from 'lucide-react'

function App() {
  const [focusedButtonIndex, setFocusedButtonIndex] = useState(0)

  const setFocus = (index: number) => {
    setFocusedButtonIndex(index)
    /** Use native browser API to query for the correct action button and focus it */
    const actionButton = document.getElementById(`action-button-${index}`)
    if (actionButton) {
      actionButton.focus()
    }
  }

  const keybinds = [
    {
      id: 'arrow-down',
      keyMatcher: (event: KeyboardEvent) => event.key === 'ArrowDown',
      callback: () => setFocus((focusedButtonIndex + 1) % 4),
    },
    {
      id: 'arrow-up',
      keyMatcher: (event: KeyboardEvent) => event.key === 'ArrowUp',
      callback: () => setFocus((focusedButtonIndex - 1 + 4) % 4),
    },
    {
      id: 'view-open-tasks',
      keyMatcher: (event: KeyboardEvent) => event.metaKey && event.key === 'v',
      callback: () => console.log('View Open Tasks'),
    },
    {
      id: 'add-task',
      keyMatcher: (event: KeyboardEvent) => event.metaKey && event.key === 't',
      callback: () => console.log('Add Task'),
    },
    {
      id: 'add-note',
      keyMatcher: (event: KeyboardEvent) => event.metaKey && event.key === 'n',
      callback: () => console.log('Add Note'),
    },
    {
      id: 'daily-note',
      keyMatcher: (event: KeyboardEvent) => event.metaKey && event.key === 'd',
      callback: () => console.log('Daily Note'),
    },
  ]

  useKeybinds(keybinds)

  /** On page load, focus the first button */
  useEffect(() => setFocus(0), [])

  return (
    <div className='min-h-screen flex items-center justify-center bg-background'>
      <div className='text-center'>
        <div className='flex flex-col gap-2 w-80'>
          <ActionButton
            id='action-button-0'
            icon={<Eye />}
            keybind={keybinds.find((keybind) => keybind.id === 'view-open-tasks')}
            to='/tasks'
          >
            View Open Tasks
          </ActionButton>
          <ActionButton
            id='action-button-1'
            icon={<Pencil />}
            keybind={keybinds.find((keybind) => keybind.id === 'add-task')}
            to='/tasks/new'
          >
            Add Task
          </ActionButton>
          <ActionButton
            id='action-button-2'
            icon={<PenIcon />}
            keybind={keybinds.find((keybind) => keybind.id === 'add-note')}
            to='/notes'
          >
            Add Note
          </ActionButton>
          <ActionButton
            id='action-button-3'
            icon={<NotebookPen />}
            keybind={keybinds.find((keybind) => keybind.id === 'daily-note')}
            // TODO: this should be a dynamic route with the current date? or that page can handle fetching the right note.
            to='/daily-note'
          >
            Daily Note
          </ActionButton>
        </div>
      </div>
    </div>
  )
}

export default App
