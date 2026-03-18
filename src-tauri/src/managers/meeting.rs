use crate::audio_toolkit::vad::SmoothedVad;
#[cfg(target_os = "macos")]
use crate::audio_toolkit::SystemAudioCapture;
use crate::audio_toolkit::{
    audio::mixer::{mix_audio, DiskAudioWriter},
    AudioRecorder, SileroVad,
};
use crate::settings::{get_settings, AppSettings};

use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

/* ────────────────────────────────────── state machine ─────────────────── */

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MeetingState {
    Idle,
    Recording,
    Paused,
    Stopped,
    Processing,
    Done,
}

impl std::fmt::Display for MeetingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeetingState::Idle => write!(f, "idle"),
            MeetingState::Recording => write!(f, "recording"),
            MeetingState::Paused => write!(f, "paused"),
            MeetingState::Stopped => write!(f, "stopped"),
            MeetingState::Processing => write!(f, "processing"),
            MeetingState::Done => write!(f, "done"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct MeetingStateEvent {
    pub state: MeetingState,
    pub title: Option<String>,
    pub duration_secs: Option<f64>,
    pub error: Option<String>,
}

/* ────────────────────────────────────── inner state ───────────────────── */

struct MeetingInner {
    state: MeetingState,
    title: Option<String>,
    started_at: Option<Instant>,
    /// Accumulated recording duration (excluding paused time).
    accumulated_secs: f64,
    /// When the current recording segment started (reset on resume).
    segment_start: Option<Instant>,

    // Audio components — created on start, torn down on stop/cancel.
    recorder: Option<AudioRecorder>,
    #[cfg(target_os = "macos")]
    system_capture: Option<SystemAudioCapture>,
    disk_writer: Option<DiskAudioWriter>,
}

impl MeetingInner {
    fn new() -> Self {
        Self {
            state: MeetingState::Idle,
            title: None,
            started_at: None,
            accumulated_secs: 0.0,
            segment_start: None,
            recorder: None,
            #[cfg(target_os = "macos")]
            system_capture: None,
            disk_writer: None,
        }
    }

    fn elapsed_secs(&self) -> f64 {
        let current = self
            .segment_start
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or(0.0);
        self.accumulated_secs + current
    }
}

/* ────────────────────────────────────── manager ───────────────────────── */

#[derive(Clone)]
pub struct MeetingManager {
    inner: Arc<Mutex<MeetingInner>>,
    app_handle: AppHandle,
}

impl MeetingManager {
    pub fn new(app: &AppHandle) -> Self {
        Self {
            inner: Arc::new(Mutex::new(MeetingInner::new())),
            app_handle: app.clone(),
        }
    }

    /* ── public queries ──────────────────────────────────────────────── */

    pub fn state(&self) -> MeetingState {
        self.inner.lock().unwrap().state.clone()
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.inner.lock().unwrap().state, MeetingState::Recording)
    }

    pub fn is_active(&self) -> bool {
        let s = self.inner.lock().unwrap().state.clone();
        matches!(s, MeetingState::Recording | MeetingState::Paused)
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.inner.lock().unwrap().elapsed_secs()
    }

    /* ── lifecycle: start ────────────────────────────────────────────── */

    pub fn start(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();

        if inner.state != MeetingState::Idle && inner.state != MeetingState::Done {
            return Err(anyhow!("Cannot start meeting from state: {}", inner.state));
        }

        // Set up microphone recorder
        let vad_path = self
            .app_handle
            .path()
            .resolve(
                "resources/models/silero_vad_v4.onnx",
                tauri::path::BaseDirectory::Resource,
            )
            .map_err(|e| anyhow!("Failed to resolve VAD path: {}", e))?;

        let silero = SileroVad::new(vad_path.to_str().unwrap(), 0.3)
            .map_err(|e| anyhow!("Failed to create SileroVad: {}", e))?;
        let smoothed_vad = SmoothedVad::new(Box::new(silero), 15, 15, 2);

        let app_handle = self.app_handle.clone();
        let mut recorder = AudioRecorder::new()
            .map_err(|e| anyhow!("Failed to create AudioRecorder: {}", e))?
            .with_vad(Box::new(smoothed_vad))
            .with_level_callback(move |levels| {
                let _ = app_handle.emit("mic-level", &levels);
            });

        // Get device from settings
        let settings = get_settings(&self.app_handle);
        let device = resolve_microphone_device(&settings);
        recorder
            .open(device)
            .map_err(|e| anyhow!("Failed to open recorder: {}", e))?;

        // Set up system audio capture (macOS only)
        #[cfg(target_os = "macos")]
        let mut system_capture = {
            let mut cap = SystemAudioCapture::new();
            match cap.open() {
                Ok(()) => {
                    info!("System audio capture opened");
                    Some(cap)
                }
                Err(e) => {
                    warn!("System audio capture unavailable: {e} — mic-only mode");
                    None
                }
            }
        };

        // Set up disk writer
        let recordings_dir = self
            .app_handle
            .path()
            .app_data_dir()
            .map(|d| d.join("recordings"))
            .map_err(|e| anyhow!("Failed to resolve recordings dir: {}", e))?;

        let disk_writer = DiskAudioWriter::new(&recordings_dir)
            .map_err(|e| anyhow!("Failed to create DiskAudioWriter: {}", e))?;

        // Start recording
        recorder
            .start()
            .map_err(|e| anyhow!("Failed to start mic recording: {}", e))?;

        #[cfg(target_os = "macos")]
        if let Some(ref mut cap) = system_capture {
            if let Err(e) = cap.start() {
                warn!("Failed to start system audio capture: {e}");
            }
        }

        // Commit state
        inner.state = MeetingState::Recording;
        inner.title = None;
        inner.started_at = Some(Instant::now());
        inner.accumulated_secs = 0.0;
        inner.segment_start = Some(Instant::now());
        inner.recorder = Some(recorder);
        #[cfg(target_os = "macos")]
        {
            inner.system_capture = system_capture;
        }
        inner.disk_writer = Some(disk_writer);

        drop(inner);
        self.emit_state_event(None);
        info!("Meeting started");
        Ok(())
    }

    /* ── lifecycle: pause ────────────────────────────────────────────── */

    pub fn pause(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();

        if inner.state != MeetingState::Recording {
            return Err(anyhow!("Cannot pause meeting from state: {}", inner.state));
        }

        // Accumulate elapsed time from this segment
        if let Some(seg) = inner.segment_start.take() {
            inner.accumulated_secs += seg.elapsed().as_secs_f64();
        }

        // Stop audio capture but keep resources alive
        let mic_samples = if let Some(ref rec) = inner.recorder {
            rec.stop().unwrap_or_default()
        } else {
            Vec::new()
        };

        #[cfg(target_os = "macos")]
        let sys_samples = if let Some(ref cap) = inner.system_capture {
            cap.stop().unwrap_or_default()
        } else {
            Vec::new()
        };
        #[cfg(not(target_os = "macos"))]
        let sys_samples: Vec<f32> = Vec::new();

        // Mix and write to disk
        let mixed = mix_audio(&mic_samples, &sys_samples);
        if let Some(ref mut writer) = inner.disk_writer {
            if let Err(e) = writer.push(&mixed) {
                error!("Failed to write mixed audio to disk: {e}");
            }
        }

        inner.state = MeetingState::Paused;
        drop(inner);
        self.emit_state_event(None);
        info!("Meeting paused");
        Ok(())
    }

    /* ── lifecycle: resume ───────────────────────────────────────────── */

    pub fn resume(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();

        if inner.state != MeetingState::Paused {
            return Err(anyhow!("Cannot resume meeting from state: {}", inner.state));
        }

        // Restart audio capture
        if let Some(ref rec) = inner.recorder {
            rec.start()
                .map_err(|e| anyhow!("Failed to resume mic recording: {}", e))?;
        }

        #[cfg(target_os = "macos")]
        if let Some(ref cap) = inner.system_capture {
            if let Err(e) = cap.start() {
                warn!("Failed to resume system audio capture: {e}");
            }
        }

        inner.segment_start = Some(Instant::now());
        inner.state = MeetingState::Recording;
        drop(inner);
        self.emit_state_event(None);
        info!("Meeting resumed");
        Ok(())
    }

    /* ── lifecycle: stop ─────────────────────────────────────────────── */

    /// Stop recording and finalize audio to disk. Returns the WAV file path.
    pub fn stop(&self) -> Result<PathBuf> {
        let mut inner = self.inner.lock().unwrap();

        if inner.state != MeetingState::Recording && inner.state != MeetingState::Paused {
            return Err(anyhow!("Cannot stop meeting from state: {}", inner.state));
        }

        // If recording (not already paused), stop audio and flush
        if inner.state == MeetingState::Recording {
            if let Some(seg) = inner.segment_start.take() {
                inner.accumulated_secs += seg.elapsed().as_secs_f64();
            }

            let mic_samples = if let Some(ref rec) = inner.recorder {
                rec.stop().unwrap_or_default()
            } else {
                Vec::new()
            };

            #[cfg(target_os = "macos")]
            let sys_samples = if let Some(ref cap) = inner.system_capture {
                cap.stop().unwrap_or_default()
            } else {
                Vec::new()
            };
            #[cfg(not(target_os = "macos"))]
            let sys_samples: Vec<f32> = Vec::new();

            let mixed = mix_audio(&mic_samples, &sys_samples);
            if let Some(ref mut writer) = inner.disk_writer {
                if let Err(e) = writer.push(&mixed) {
                    error!("Failed to write final mixed audio to disk: {e}");
                }
            }
        }

        // Close audio resources
        if let Some(mut rec) = inner.recorder.take() {
            let _ = rec.close();
        }
        #[cfg(target_os = "macos")]
        if let Some(mut cap) = inner.system_capture.take() {
            let _ = cap.close();
        }

        // Finalize the WAV file
        let path = if let Some(writer) = inner.disk_writer.take() {
            let duration = writer.duration_secs();
            info!("Meeting audio: {:.1}s written to disk", duration);
            writer
                .finish()
                .map_err(|e| anyhow!("Failed to finalize WAV: {}", e))?
        } else {
            return Err(anyhow!("No disk writer available"));
        };

        inner.state = MeetingState::Stopped;
        drop(inner);
        self.emit_state_event(None);
        info!("Meeting stopped — audio at {}", path.display());
        Ok(path)
    }

    /* ── lifecycle: set title (after stop) ────────────────────────────── */

    pub fn set_title(&self, title: String) {
        let mut inner = self.inner.lock().unwrap();
        inner.title = Some(title.clone());
        drop(inner);
        debug!("Meeting title set: {title}");
        self.emit_state_event(None);
    }

    /* ── lifecycle: processing ───────────────────────────────────────── */

    pub fn begin_processing(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.state != MeetingState::Stopped {
            return Err(anyhow!(
                "Cannot begin processing from state: {}",
                inner.state
            ));
        }
        inner.state = MeetingState::Processing;
        drop(inner);
        self.emit_state_event(None);
        info!("Meeting processing started");
        Ok(())
    }

    pub fn finish_processing(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.state != MeetingState::Processing {
            return Err(anyhow!(
                "Cannot finish processing from state: {}",
                inner.state
            ));
        }
        inner.state = MeetingState::Done;
        drop(inner);
        self.emit_state_event(None);
        info!("Meeting processing complete");
        Ok(())
    }

    /* ── lifecycle: cancel ───────────────────────────────────────────── */

    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();

        match inner.state {
            MeetingState::Recording | MeetingState::Paused => {
                // Stop and discard audio
                if inner.state == MeetingState::Recording {
                    if let Some(ref rec) = inner.recorder {
                        let _ = rec.stop();
                    }
                    #[cfg(target_os = "macos")]
                    if let Some(ref cap) = inner.system_capture {
                        let _ = cap.stop();
                    }
                }

                if let Some(mut rec) = inner.recorder.take() {
                    let _ = rec.close();
                }
                #[cfg(target_os = "macos")]
                if let Some(mut cap) = inner.system_capture.take() {
                    let _ = cap.close();
                }

                // Clean up the temp WAV file
                if let Some(writer) = inner.disk_writer.take() {
                    let path = writer.path().to_path_buf();
                    drop(writer);
                    if path.exists() {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
            MeetingState::Stopped | MeetingState::Processing => {
                // Already stopped; just reset state
            }
            _ => return,
        }

        inner.state = MeetingState::Idle;
        inner.title = None;
        inner.started_at = None;
        inner.accumulated_secs = 0.0;
        inner.segment_start = None;
        drop(inner);
        self.emit_state_event(None);
        info!("Meeting cancelled");
    }

    /* ── lifecycle: reset (after Done) ───────────────────────────────── */

    pub fn reset(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.state != MeetingState::Done && inner.state != MeetingState::Idle {
            warn!("reset() called in state {}; forcing idle", inner.state);
        }
        *inner = MeetingInner::new();
        drop(inner);
        self.emit_state_event(None);
    }

    /* ── event emission ──────────────────────────────────────────────── */

    fn emit_state_event(&self, error: Option<String>) {
        let inner = self.inner.lock().unwrap();
        let event = MeetingStateEvent {
            state: inner.state.clone(),
            title: inner.title.clone(),
            duration_secs: Some(inner.elapsed_secs()),
            error,
        };
        drop(inner);

        if let Err(e) = self.app_handle.emit("meeting-state", &event) {
            error!("Failed to emit meeting-state event: {e}");
        }
    }
}

/* ────────────────────────────────────── helpers ───────────────────────── */

fn resolve_microphone_device(settings: &AppSettings) -> Option<cpal::Device> {
    use crate::audio_toolkit::list_input_devices;
    use crate::helpers::clamshell;

    let use_clamshell_mic =
        clamshell::is_clamshell().unwrap_or(false) && settings.clamshell_microphone.is_some();

    let device_name = if use_clamshell_mic {
        settings.clamshell_microphone.as_ref().unwrap()
    } else {
        settings.selected_microphone.as_ref()?
    };

    match list_input_devices() {
        Ok(devices) => devices
            .into_iter()
            .find(|d| d.name == *device_name)
            .map(|d| d.device),
        Err(e) => {
            debug!("Failed to list devices, using default: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_display() {
        assert_eq!(MeetingState::Idle.to_string(), "idle");
        assert_eq!(MeetingState::Recording.to_string(), "recording");
        assert_eq!(MeetingState::Paused.to_string(), "paused");
        assert_eq!(MeetingState::Stopped.to_string(), "stopped");
        assert_eq!(MeetingState::Processing.to_string(), "processing");
        assert_eq!(MeetingState::Done.to_string(), "done");
    }

    #[test]
    fn state_serialization() {
        assert_eq!(
            serde_json::to_string(&MeetingState::Recording).unwrap(),
            "\"recording\""
        );
        assert_eq!(
            serde_json::to_string(&MeetingState::Paused).unwrap(),
            "\"paused\""
        );
    }
}
