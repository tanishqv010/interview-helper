# Interview Helper

React + Tauri desktop app that captures screenshots and sends them to a Gemini model for analysis, rendering the response as Markdown. Includes global shortcuts for queueing captures and sending them in one shot.

## Features
- Screenshot queue: capture multiple screens and send them together for multi-image reasoning
- Global shortcuts: work from anywhere without focusing the app
- Prompt templates: switch between MCQ generators, code-only, or code-with-explanation
- Model selection: choose the Gemini model at runtime
- Language hinting: add a preferred programming language to the prompt
- Markdown output: rich rendering with a one-click “Copy” button on code blocks
- Lightweight UI overlay: adjustable opacity and quick show/hide
- Visibility-aware overlay: designed to be excluded from standard desktop screenshots and most meeting-app screen sharing (e.g., Google Meet, Zoom), within platform limits

## Interface
- Prompt editor: large textarea to customize the request sent to the model
- Controls bar:
  - Ask (queue count): sends all queued screenshots to the model
  - Add Image (H): captures the current screen to the queue
  - Clear (R): clears output and resets the queue length
  - Set Key: set/update your Gemini API key from within the app
  - Model select: switch between models like `gemini-2.5-pro` and `gemini-2.5-flash`
  - Language select: hint the preferred language (C++, Python, Java, etc.)
  - Output format select: choose between single/multiple-correct MCQ, code-only, or code-with-explanation
- Output panel: Markdown-rendered response with a copy button for code blocks
- Footer: lists all available global shortcuts for quick reference

## Visibility and screen sharing
- The overlay is created using window flags that typically exclude it from OS-level screenshots and from the capture pipeline used by common meeting apps (Google Meet, Zoom, etc.).
- Behavior can vary by OS, driver, capture method, or app updates. If visibility is critical, test with your setup. You can toggle visibility with `Ctrl+Shift+B`.

## Prerequisites
- Node.js 18+ (or Bun)
- Rust toolchain (stable) and Tauri prerequisites for your OS
- A Gemini API key

## Quick start
   ```bash
# clone
git clone <your-repo-url>
cd interview_helper

# install npm dependencies
   npm install
# or: bun install

# install rust dependencies
   cd src-tauri
   cargo build
   cd ..

# set env (create src-tauri/.env)
# GEMINI_API_KEY=your_key
# GEMINI_MODEL=gemini-2.5-pro

# run dev
   npm run tauri dev

# build installer
   npm run tauri build
   ```

## Keyboard shortcuts
- Ctrl+Shift+H: Add current screen to image queue
- Ctrl+Shift+Enter: Send queued images to the model
- Ctrl+Shift+R: Clear queue
- Ctrl+Shift+Arrow Keys: Nudge window
- Ctrl+Shift+\]: Increase opacity
- Ctrl+Shift+\[: Decrease opacity
- Ctrl+Shift+B: Toggle visibility
- Ctrl+Shift+Q: Quit

## Configuration
- Tauri config: `src-tauri/tauri.conf.json`
- Env: create `src-tauri/.env` (see variables above). You can also set environment variables globally.
- Model can be changed at runtime via the UI. Supported defaults include `gemini-2.5-pro` and fast variants.

## Development notes
- Frontend: React + Vite + Tailwind
- Backend: Rust + Tauri 2
- Markdown rendering: `react-markdown`

