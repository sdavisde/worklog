import { createBrowserRouter, RouterProvider } from 'react-router'

import React from 'react'
import ReactDOM from 'react-dom/client'
import './globals.css'
import App from './App'
import { useKeybinds } from './lib/use-keybinds'
import { invoke } from '@tauri-apps/api/core'
import { ErrorBoundary } from './error'

function GlobalKeybinds() {
  useKeybinds([
    {
      keyMatcher: (event: KeyboardEvent) => event.key === 'Escape',
      callback: () => {
        invoke('hide_main_window')
      },
    },
  ])
  return null
}

const router = createBrowserRouter([
  {
    path: '/',
    element: <App />,
    ErrorBoundary,
  },
])

const root = document.getElementById('root')!

if (!root) {
  throw new Error('Root element not found')
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <GlobalKeybinds />
    <RouterProvider router={router} />
  </React.StrictMode>
)
