use crate::audio_toolkit::constants;

/// Mix two mono f32 audio buffers (both at WHISPER_SAMPLE_RATE) into one.
///
/// Shorter buffer is zero-padded to match the longer one.
/// Samples are summed and clamped to [-1.0, 1.0].
pub fn mix_audio(mic: &[f32], system: &[f32]) -> Vec<f32> {
    let len = mic.len().max(system.len());
    let mut out = Vec::with_capacity(len);

    for i in 0..len {
        let m = mic.get(i).copied().unwrap_or(0.0);
        let s = system.get(i).copied().unwrap_or(0.0);
        out.push((m + s).clamp(-1.0, 1.0));
    }

    out
}

/// Streams mixed audio chunks to a temporary WAV file on disk.
///
/// Designed for long meetings where holding all audio in memory is impractical.
/// Call `push()` to append chunks, `finish()` to flush and get the path.
pub struct DiskAudioWriter {
    path: std::path::PathBuf,
    writer: Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>,
    samples_written: u64,
}

impl DiskAudioWriter {
    pub fn new(dir: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(dir)?;

        let filename = format!(
            "yad_recording_{}.wav",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let path = dir.join(filename);

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: constants::WHISPER_SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let writer = hound::WavWriter::create(&path, spec)?;

        Ok(Self {
            path,
            writer: Some(writer),
            samples_written: 0,
        })
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut w) = self.writer {
            for &s in samples {
                w.write_sample(s)?;
            }
            self.samples_written += samples.len() as u64;
        }
        Ok(())
    }

    pub fn samples_written(&self) -> u64 {
        self.samples_written
    }

    pub fn duration_secs(&self) -> f64 {
        self.samples_written as f64 / constants::WHISPER_SAMPLE_RATE as f64
    }

    pub fn finish(mut self) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        if let Some(w) = self.writer.take() {
            w.finalize()?;
        }
        Ok(self.path.clone())
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for DiskAudioWriter {
    fn drop(&mut self) {
        if let Some(w) = self.writer.take() {
            let _ = w.finalize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_equal_length() {
        let mic = vec![0.5, -0.3, 0.0];
        let sys = vec![0.2, 0.4, -0.1];
        let out = mix_audio(&mic, &sys);
        assert_eq!(out.len(), 3);
        assert!((out[0] - 0.7).abs() < 1e-6);
        assert!((out[1] - 0.1).abs() < 1e-6);
        assert!((out[2] - (-0.1)).abs() < 1e-6);
    }

    #[test]
    fn mix_unequal_length_pads_shorter() {
        let mic = vec![0.5, 0.5];
        let sys = vec![0.3];
        let out = mix_audio(&mic, &sys);
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.8).abs() < 1e-6);
        assert!((out[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn mix_clamps_to_range() {
        let mic = vec![0.9];
        let sys = vec![0.9];
        let out = mix_audio(&mic, &sys);
        assert_eq!(out[0], 1.0);

        let mic = vec![-0.8];
        let sys = vec![-0.8];
        let out = mix_audio(&mic, &sys);
        assert_eq!(out[0], -1.0);
    }

    #[test]
    fn mix_empty_inputs() {
        assert!(mix_audio(&[], &[]).is_empty());
        let out = mix_audio(&[0.5], &[]);
        assert_eq!(out, vec![0.5]);
    }

    #[test]
    fn disk_writer_roundtrip() {
        let dir = std::env::temp_dir().join("yad_test_mixer");
        let _ = std::fs::remove_dir_all(&dir);

        let mut writer = DiskAudioWriter::new(&dir).unwrap();
        writer.push(&[0.1, 0.2, 0.3]).unwrap();
        writer.push(&[0.4, 0.5]).unwrap();
        assert_eq!(writer.samples_written(), 5);

        let path = writer.finish().unwrap();
        assert!(path.exists());

        let reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, constants::WHISPER_SAMPLE_RATE);
        assert_eq!(spec.sample_format, hound::SampleFormat::Float);

        let samples: Vec<f32> = reader.into_samples::<f32>().map(|s| s.unwrap()).collect();
        assert_eq!(samples.len(), 5);
        assert!((samples[0] - 0.1).abs() < 1e-6);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
