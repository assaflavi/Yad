// Re-export all audio components
mod device;
pub mod mixer;
mod recorder;
mod resampler;
#[cfg(target_os = "macos")]
mod system_audio;
mod utils;
mod visualizer;

pub use device::{list_input_devices, list_output_devices, CpalDeviceInfo};
pub use recorder::{is_microphone_access_denied, AudioRecorder};
pub use resampler::FrameResampler;
#[cfg(target_os = "macos")]
pub use system_audio::{
    is_screen_recording_permitted, request_screen_recording_permission, SystemAudioCapture,
};
pub use utils::save_wav_file;
pub use visualizer::AudioVisualiser;
