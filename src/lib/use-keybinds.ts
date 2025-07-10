import { useEffect } from 'react'

export type KeybindConfig = {
  keyMatcher: (event: KeyboardEvent) => boolean
  callback: () => void
}

/**
 * A hook that listens for a specific keyboard event and triggers a callback function when the event is triggered.
 * @param keyMatcher - A function that takes a KeyboardEvent and returns true if the event matches the keybind.
 * @param callback - The function to trigger when the key is pressed.
 */
export const useKeybinds = (keybinds: KeybindConfig[]) => {
  useEffect(() => {
    /** Need to handle combinations */
    const handleKeydown = (event: KeyboardEvent) => {
      keybinds.forEach((keybind) => {
        if (keybind.keyMatcher(event)) {
          keybind.callback()
        }
      })
    }

    window.addEventListener('keydown', handleKeydown)

    return () => {
      window.removeEventListener('keydown', handleKeydown)
    }
  }, [keybinds])
}
