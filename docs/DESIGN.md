# Yad — Meeting Transcription & Summarization for macOS

**Yad** (יד — Hebrew for "hand") is a macOS desktop app for meeting transcription and summarization. Forked from [Handy](https://github.com/cjpais/handy), it captures both sides of a remote meeting, transcribes locally using Whisper, and produces structured Markdown summaries via a cloud LLM.

The name references the _yad_, the pointer used to follow text when reading Torah — a metaphor for an app that follows along a conversation and produces a written record. It's also a nod to the upstream project, Handy.

## Core Pipeline

```
System Audio (ScreenCaptureKit) + Microphone (CPAL)
        │                              │
        └──────── Mix ─────────────────┘
                   │
                   ▼
         Audio Buffer (stream to disk)
                   │  on stop
                   ▼
         Whisper (local, any language)
                   │  raw transcript
                   ▼
         Cloud LLM (user-configured provider)
         + Markdown template engine
                   │  formatted output
                   ▼
         .md file (local folder / clipboard)
         transcript + summary preserved
         no audio stored
```

## MVP Scope

### Audio Capture

- **System audio** via `screencapturekit` Rust crate (macOS 13+)
- **Microphone** via CPAL (existing Handy infrastructure)
- **Mixed single stream** for MVP — both sources merged before transcription
- Audio streamed to disk in chunks (temp WAV segments) for long meetings
- No audio stored after transcription — privacy-first

### Transcription

- Local Whisper inference via `transcribe-rs` (existing Handy infrastructure)
- Full model catalog preserved: Whisper Small/Medium/Turbo/Large, Parakeet V2/V3, Moonshine, SenseVoice, GigaAM, Canary
- Any source language supported (model-dependent)
- Single-pass transcription of full concatenated audio after recording stops

### Summarization

- Cloud LLM post-processing (existing Handy infrastructure)
- User configures provider (OpenAI, Anthropic, custom OpenAI-compatible), API key, and model
- Summary always produced in the organization's documentation language (configurable, defaults to English)
- Customizable Markdown templates with `%%placeholders%%`

### Template Engine

Available placeholders:

- `%%title%%` — user-provided or default timestamp
- `%%date%%` — meeting date (formatted per locale)
- `%%duration%%` — recording duration
- `%%start_time%%`, `%%end_time%%`
- `%%summary%%` — LLM-generated summary
- `%%decisions%%` — LLM-generated key decisions
- `%%action_items%%` — LLM-generated action items
- `%%transcript%%` — full raw transcript
- `%%language%%` — source language detected/selected

Default title: human-readable lexicographical timestamp of meeting start (e.g., `2026-03-18 14.30`).

Default template:

```markdown
# %%title%%

**Date:** %%date%%
**Duration:** %%duration%%

## Summary

%%summary%%

## Key Decisions

%%decisions%%

## Action Items

%%action_items%%

---

<details>
<summary>Full Transcript</summary>

%%transcript%%

</details>
```

Filename pattern: `%%date%%-%%title%%.md` (configurable).

### Meeting Lifecycle

States: `Idle → Recording → Paused → Recording → ... → Stopped → TitlePrompt → Processing → Done`

Controls:

- **Start:** Global hotkey or tray menu
- **Pause/Resume:** Tray menu or hotkey
- **Stop:** Tray menu or hotkey → prompts for title
- **Cancel:** Discard session

### UX

- **Tray-only** during meetings — no notepad, no overlay
- Tray icon states: idle, recording (pulsing), processing
- Tray menu shows recording duration and controls during a session
- **Title prompt** after stopping: small window with text input (pre-filled with timestamp), template picker, "Summarize" button, "Save transcript only" option
- **Settings window** (from tray): General, Audio, Models, Summarization (provider/key/templates), History, Shortcuts
- **Processing indicator:** "Transcribing..." → "Summarizing..." → "Done ✓ — Open"

### Onboarding (first launch)

1. Grant Screen Recording permission (ScreenCaptureKit)
2. Grant Microphone permission
3. Download a Whisper model
4. Configure LLM provider + API key
5. Set output folder

### Output

- Save `.md` files to user-configured folder (e.g., `~/Meeting Notes/` or Obsidian vault)
- Optionally copy to clipboard
- Transcript + summary preserved in the same file
- No audio stored

### Permissions (macOS)

- Screen Recording (for ScreenCaptureKit audio capture)
- Microphone access
- `NSScreenCaptureUsageDescription` and `NSMicrophoneUsageDescription` in Info.plist

## Reuse from Handy

| Component                                  | Reuse                                      |
| ------------------------------------------ | ------------------------------------------ |
| Tauri shell, build system, single-instance | ✅ As-is                                   |
| Model manager (download, load, unload)     | ✅ As-is                                   |
| Whisper/transcribe-rs integration          | ✅ As-is                                   |
| LLM post-processing pipeline               | ✅ Adapt prompts for meeting summarization |
| CPAL audio recording (mic)                 | ✅ As-is                                   |
| VAD (Silero)                               | ✅ For silence detection, not filtering    |
| Tray menu infrastructure                   | ✅ Reshape for meeting controls            |
| Global hotkey system                       | ✅ Remap for start/pause/stop              |
| Settings persistence                       | ✅ Extend schema                           |

## New Components

| Component            | Description                                                     |
| -------------------- | --------------------------------------------------------------- |
| `SystemAudioCapture` | ScreenCaptureKit integration via `screencapturekit` crate       |
| `MeetingManager`     | Session lifecycle (start/pause/resume/stop/title/process)       |
| `TemplateEngine`     | Markdown templates with `%%placeholders%%`                      |
| `MeetingStore`       | Persists transcript + summary as `.md` files                    |
| Frontend             | Title dialog, settings, meeting history (simplified from Handy) |

## Roadmap (Post-MVP)

1. **Real-time transcript feedback** — Incremental transcription on silence/pause, live transcript view. Final output still from full audio.
2. **Speaker attribution** — Separate transcription of system audio vs mic, label "them" / "me" in transcript.
3. **Calendar integration** — macOS Calendar / Google Calendar. Auto-detect meetings, pre-fill metadata (title, participants, duration).
4. **Notepad mode** — Granola-style scratchpad. User notes merged with transcript by LLM.
5. **Obsidian integration** — Plugin or URI scheme for linking, tagging, backlinks.
6. **Cross-platform** — Windows (WASAPI loopback), Linux (PipeWire monitor).
7. **In-room capture** — Physical meeting mode with single mic, possibly multi-speaker diarization.

## Technology Stack

| Layer          | Technology                                            |
| -------------- | ----------------------------------------------------- |
| App framework  | Tauri 2.x (Rust backend + WebView frontend)           |
| Frontend       | React + TypeScript + Tailwind CSS                     |
| Audio (mic)    | CPAL (CoreAudio on macOS)                             |
| Audio (system) | `screencapturekit` Rust crate                         |
| Resampling     | rubato                                                |
| VAD            | Silero VAD (ONNX)                                     |
| Transcription  | whisper-rs, transcribe-rs (local inference)           |
| Summarization  | OpenAI / Anthropic / custom provider APIs             |
| Storage        | tauri-plugin-store (settings), filesystem (.md files) |
| Packaging      | Tauri bundler (macOS .dmg)                            |

## Key Design Decisions

1. **Fork, not extend** — Handy's dictation UX and meeting capture are different enough in lifecycle, audio pipeline, and UI to warrant a separate app.
2. **Mixed audio stream for MVP** — Merging system + mic audio before transcription. Speaker attribution deferred.
3. **Local transcription, cloud summarization** — Best balance of privacy (audio never leaves device) and quality (cloud LLMs for summarization).
4. **No audio storage** — Privacy-first, matching Granola's model.
5. **Template-driven output** — Users control the summary structure via `%%placeholder%%` templates.
6. **macOS-first** — ScreenCaptureKit dependency. Cross-platform deferred.
7. **All models preserved** — English-only models (Parakeet V2) remain useful for English-speaking orgs.
