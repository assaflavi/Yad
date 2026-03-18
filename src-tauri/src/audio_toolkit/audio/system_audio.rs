use std::sync::{mpsc, Mutex};

use screencapturekit::prelude::*;
use screencapturekit::stream::delegate_trait::StreamCallbacks;

use crate::audio_toolkit::{audio::FrameResampler, constants};

/// Check if Screen Recording permission is granted (non-prompting).
pub fn is_screen_recording_permitted() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Prompt the user to grant Screen Recording permission.
/// Returns `true` if access was granted.
pub fn request_screen_recording_permission() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

enum Cmd {
    Start,
    Stop(mpsc::Sender<Vec<f32>>),
    Shutdown,
}

pub struct SystemAudioCapture {
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
}

impl SystemAudioCapture {
    pub fn new() -> Self {
        SystemAudioCapture {
            cmd_tx: None,
            worker_handle: None,
        }
    }

    pub fn open(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.worker_handle.is_some() {
            return Ok(());
        }

        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<(), String>>(1);

        let worker = std::thread::spawn(move || {
            let init_result = (|| -> Result<(SCStream, mpsc::Receiver<Vec<f32>>), String> {
                if !is_screen_recording_permitted() {
                    return Err(
                        "Screen Recording permission denied. Grant access in System Settings."
                            .to_string(),
                    );
                }

                let content = SCShareableContent::get()
                    .map_err(|e| format!("Failed to get shareable content: {e}"))?;

                let display = content
                    .displays()
                    .into_iter()
                    .next()
                    .ok_or_else(|| "No display found for ScreenCaptureKit".to_string())?;

                let filter = SCContentFilter::create()
                    .with_display(&display)
                    .with_excluding_windows(&[])
                    .build();

                let config = SCStreamConfiguration::new()
                    .with_width(2)
                    .with_height(2)
                    .with_captures_audio(true)
                    .with_sample_rate(constants::SCK_SAMPLE_RATE as i32)
                    .with_channel_count(1)
                    .with_excludes_current_process_audio(true);

                let (sample_tx, sample_rx) = mpsc::channel::<Vec<f32>>();

                let delegate = StreamCallbacks::new()
                    .on_error(|e| log::error!("ScreenCaptureKit stream error: {e}"))
                    .on_stop(|e| {
                        if let Some(msg) = e {
                            log::warn!("ScreenCaptureKit stream stopped: {msg}");
                        }
                    });

                let mut stream = SCStream::new_with_delegate(&filter, &config, delegate);

                let handler = AudioOutputHandler {
                    sample_tx: Mutex::new(sample_tx),
                };
                stream.add_output_handler(handler, SCStreamOutputType::Audio);

                stream
                    .start_capture()
                    .map_err(|e| format!("Failed to start capture: {e}"))?;

                Ok((stream, sample_rx))
            })();

            match init_result {
                Ok((stream, sample_rx)) => {
                    let _ = init_tx.send(Ok(()));
                    run_consumer(sample_rx, cmd_rx);
                    let _ = stream.stop_capture();
                    drop(stream);
                }
                Err(msg) => {
                    log::error!("SystemAudioCapture init failed: {msg}");
                    let _ = init_tx.send(Err(msg));
                }
            }
        });

        match init_rx.recv() {
            Ok(Ok(())) => {
                self.cmd_tx = Some(cmd_tx);
                self.worker_handle = Some(worker);
                Ok(())
            }
            Ok(Err(msg)) => {
                let _ = worker.join();
                let kind = if msg.contains("permission denied") || msg.contains("Permission denied")
                {
                    std::io::ErrorKind::PermissionDenied
                } else {
                    std::io::ErrorKind::Other
                };
                Err(Box::new(std::io::Error::new(kind, msg)))
            }
            Err(e) => {
                let _ = worker.join();
                Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("SystemAudioCapture worker init failed: {e}"),
                )))
            }
        }
    }

    pub fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Start)?;
        }
        Ok(())
    }

    pub fn stop(&self) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let (resp_tx, resp_rx) = mpsc::channel();
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Stop(resp_tx))?;
        }
        Ok(resp_rx.recv()?)
    }

    pub fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        if let Some(h) = self.worker_handle.take() {
            let _ = h.join();
        }
        Ok(())
    }
}

impl Drop for SystemAudioCapture {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

struct AudioOutputHandler {
    sample_tx: Mutex<mpsc::Sender<Vec<f32>>>,
}

impl SCStreamOutputTrait for AudioOutputHandler {
    fn did_output_sample_buffer(&self, sample_buffer: CMSampleBuffer, of_type: SCStreamOutputType) {
        if of_type != SCStreamOutputType::Audio {
            return;
        }

        let audio_buffers = match sample_buffer.audio_buffer_list() {
            Some(bufs) => bufs,
            None => return,
        };

        for buf in &audio_buffers {
            let raw_bytes = buf.data();
            if raw_bytes.len() < 4 {
                continue;
            }

            let samples: Vec<f32> = raw_bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();

            let tx = self.sample_tx.lock().unwrap();
            let _ = tx.send(samples);
        }
    }
}

fn run_consumer(sample_rx: mpsc::Receiver<Vec<f32>>, cmd_rx: mpsc::Receiver<Cmd>) {
    let mut resampler = FrameResampler::new(
        constants::SCK_SAMPLE_RATE as usize,
        constants::WHISPER_SAMPLE_RATE as usize,
        std::time::Duration::from_millis(30),
    );

    let mut processed_samples = Vec::<f32>::new();
    let mut recording = false;

    loop {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Cmd::Start => {
                    processed_samples.clear();
                    recording = true;
                }
                Cmd::Stop(reply_tx) => {
                    recording = false;
                    resampler.finish(&mut |frame: &[f32]| {
                        processed_samples.extend_from_slice(frame);
                    });
                    let _ = reply_tx.send(std::mem::take(&mut processed_samples));
                }
                Cmd::Shutdown => return,
            }
        }

        match sample_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(samples) => {
                if recording {
                    resampler.push(&samples, &mut |frame: &[f32]| {
                        processed_samples.extend_from_slice(frame);
                    });
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}
