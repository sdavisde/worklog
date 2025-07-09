# Worklog

A local-first, keyboard-driven GUI app for engineers to track their daily work, contributions, and performance insights via a Raycast-style prompt interface.

---

## 🚀 Installing Worklog

### Prerequisites

- **macOS** (MVP target)
- [Node.js](https://nodejs.org/) (v18 or newer)
- [Rust](https://www.rust-lang.org/tools/install) (for Tauri backend)
- [Tauri prerequisites](https://tauri.app/v1/guides/getting-started/prerequisites) (Xcode, etc.)

### 1. Clone the Repository

```bash
git clone <repository>
cd worklog
```

### 2. Install Dependencies

```bash
npm install
```

### 3. Build the Application

#### For Development (hot reload, debugging):

```bash
npm run tauri dev
```

#### For Production (creates a standalone `.app` and `.dmg`):

```bash
npm run tauri build
```

- The built app will be in:  
  `src-tauri/target/release/bundle/macos/worklog.app`
- The installer DMG will be in:  
  `src-tauri/target/release/bundle/dmg/worklog_0.1.0_aarch64.dmg`

Open the `.dmg` to install Worklog like any other Mac app.

---

## 🔄 Updating Worklog

To update to the latest version:

1. **Pull the latest code:**
   ```bash
   git pull origin main
   ```
2. **Update dependencies (if needed):**
   ```bash
   npm install
   ```
3. **Rebuild the app:**
   - For development:
     ```bash
     npm run tauri dev
     ```
   - For production:
     ```bash
     npm run tauri build
     ```
   - Reinstall the new `.app` or `.dmg` as before.

---

## 🗂 Data Location

All your data is stored locally in `~/.worklog/` and is preserved between updates.

---

## ⚠️ Notes

- If you see a warning about the bundle identifier ending with `.app`, you can safely ignore it for local builds. For App Store distribution, update the identifier in `src-tauri/tauri.conf.json`.
- For code signing and notarization (required for distribution outside your own machine), see the [Tauri macOS guide](https://tauri.app/v1/guides/distribution/macos/).

---

## Features

- **Local-first storage**: All data stored in `~/.worklog/` directory
- **Work entry tracking**: Add entries with titles, tags, and conditional fields
- **Tag system**: Configurable tags with auto-tagging support
- **Azure DevOps integration**: Track ticket numbers for features and bugfixes
- **Modern UI**: Clean, responsive interface built with React and Tailwind CSS

## Development

### Prerequisites

- Node.js >= 18
- Rust (for Tauri)
- macOS (for MVP)

### Setup

```bash
git clone <repository>
cd worklog
npm install
npm run tauri dev
```

### Data Storage

The app stores all data locally in `~/.worklog/`:

- `~/.worklog/worklog.csv` - Work entries
- `~/.worklog/config.json` - User configuration

## Current Status

✅ **Completed**:

- Basic Tauri + React setup
- Data service with CSV storage
- Work entry form with conditional fields
- Tag system with configurable tags
- Modern UI with Tailwind CSS

🚧 **In Progress**:

- System-wide hotkey registration
- End-of-day notifications
- Weekly summary generation
- KPI dashboard

## Project Plan

See `project-plan.md` for detailed specifications and roadmap.
