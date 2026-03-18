# Yad

**יד — macOS meeting transcription & summarization**

Yad captures both sides of a remote meeting, transcribes locally using Whisper, and produces structured Markdown summaries via a cloud LLM. Forked from [Handy](https://github.com/cjpais/handy).

The name references the *yad*, the pointer used to follow text when reading Torah — a metaphor for following along a conversation and producing a written record.

> ⚠️ **Early development** — Yad is under active development and not yet ready for general use.

## How It Works

1. **Start recording** via global hotkey or tray menu
2. **Capture audio** — system audio (ScreenCaptureKit) + microphone recorded simultaneously
3. **Transcribe locally** — on stop, full audio is transcribed via Whisper with Metal acceleration
4. **Summarize** — a cloud LLM processes the transcript using customizable Markdown templates
5. **Save output** — structured `.md` file saved to your chosen location (e.g., an Obsidian vault)

Audio is never stored after transcription — privacy first.

## Key Features

- **Local transcription** — Whisper models with Metal acceleration, runs entirely on-device
- **Cloud LLM summarization** — OpenAI, Anthropic, or custom provider
- **Customizable templates** — Markdown templates with `%%placeholders%%` for structured output
- **Tray-only UX** — stays out of your way during meetings
- **macOS only** — requires macOS 13+ (ScreenCaptureKit)
- **No audio stored** — audio is discarded after transcription

## Development

Yad is built with a **Rust backend** (Tauri 2.x) and a **React/TypeScript frontend**.

See [BUILD.md](BUILD.md) for build instructions and prerequisites.

## Architecture

See [docs/DESIGN.md](docs/DESIGN.md) for the full design specification.

## Acknowledgments

- Forked from [Handy](https://github.com/cjpais/handy) by CJ Pais — an open-source speech-to-text app
- [Whisper](https://github.com/openai/whisper) by OpenAI
- [whisper.cpp](https://github.com/ggerganov/whisper.cpp) / [ggml](https://github.com/ggerganov/ggml)
- [Silero VAD](https://github.com/snakers4/silero-vad)
- [Tauri](https://tauri.app)

## License

[MIT](LICENSE)
