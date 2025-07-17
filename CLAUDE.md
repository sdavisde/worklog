# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

**Development with hot reload:**
```bash
yarn tauri dev
```

**Production build:**
```bash
yarn tauri build
```

**Frontend only development:**
```bash
yarn dev
```

**Frontend build:**
```bash
yarn build
```

**Important:** Use `yarn` for all package management and script execution (not npm), as specified in the Cursor rules.

## Architecture Overview

Worklog is a Tauri-based desktop application that combines:
- **Frontend:** React + TypeScript + Vite + Tailwind CSS
- **Backend:** Rust with Tauri framework
- **UI Components:** Custom components built with Radix UI primitives
- **Routing:** React Router for navigation
- **Storage:** Local-first CSV storage in `~/.worklog/`

### Key Architecture Components

**Tauri Integration:**
- Global shortcut registration (Cmd+. to show/hide window)
- System tray with quit menu
- Window management commands (`show_main_window`, `hide_main_window`)
- Window configured as `alwaysOnTop` and initially `visible: false`

**Frontend Structure:**
- `src/App.tsx` - Main application with keyboard-driven UI (4 action buttons)
- `src/main.tsx` - Router setup and global keybinds (Escape to hide window)
- `src/lib/use-keybinds.ts` - Custom hook for keyboard event handling
- `src/components/ui/` - Reusable UI components (action-button, button, keybind-preview)

**Routing:**
- Main app at `/` with ActionButtons linking to various routes
- Planned routes: `/tasks`, `/tasks/new`, `/notes`, `/daily-note`

**Keybind System:**
- Arrow keys for navigation between action buttons
- Cmd+V, Cmd+T, Cmd+N, Cmd+D for quick actions
- Escape to hide window (global)
- Cmd+. to show/hide window (global shortcut)

**Path Aliases:**
- `@/*` maps to `./src/*` (configured in tsconfig.json and vite.config.ts)

## Data Storage

All data is stored locally in `~/.worklog/`:
- `worklog.csv` - Work entries
- `config.json` - User configuration

## Core Design Philosophy

**KEYBOARD-FIRST APPROACH:** This application is built with the primary goal of making task creation lightning fast using only the keyboard. Users should NEVER need to use a mouse for any functionality. This philosophy should inform all coding decisions:

- All UI interactions must be accessible via keyboard shortcuts
- Tab navigation should work seamlessly throughout the application
- Form inputs should have logical tab order and keyboard shortcuts
- Submit actions should be triggered via Enter key
- Navigation should use arrow keys where appropriate
- All buttons and interactive elements must be keyboard accessible

## Key Features

- **Raycast-style interface** - Always-on-top window with keyboard navigation
- **Global shortcuts** - Cmd+. to toggle window visibility
- **Keyboard-driven** - Full keyboard navigation and shortcuts (NO MOUSE REQUIRED)
- **Local-first** - No cloud dependencies, all data stored locally
- **Work tracking** - Entries with titles, tags, and conditional fields
- **Tag system** - Configurable tags with auto-tagging support
- **Azure DevOps integration** - Ticket number tracking for features/bugfixes

## Development Notes

- Window starts hidden and is shown via global shortcut
- Requires macOS Accessibility permissions for global shortcuts
- Built for macOS initially (MVP target)
- Uses Tauri's webview for React frontend
- Vite dev server runs on port 1420 for Tauri integration