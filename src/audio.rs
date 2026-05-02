use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[mockall::automock]
#[async_trait::async_trait]
pub trait AudioCapture: Send + Sync {
    async fn start_capture(&mut self) -> Result<()>;
    async fn stop_capture(&mut self) -> Result<Vec<u8>>;
    fn is_capturing(&self) -> bool;
    fn elapsed_seconds(&self) -> u64;
    fn device_name(&self) -> Option<String>;
}

pub struct CpalAudioCapture {
    buffer: Arc<Mutex<Vec<u8>>>,
    is_capturing: bool,
    start_time: Option<Instant>,
    stream: Option<cpal::Stream>,
    device_name: Option<String>,
}

impl CpalAudioCapture {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let device_name = host
            .default_input_device()
            .and_then(|d| d.name().ok());

        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            is_capturing: false,
            start_time: None,
            stream: None,
            device_name,
        }
    }
}

#[async_trait::async_trait]
impl AudioCapture for CpalAudioCapture {
    async fn start_capture(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device available"))?;

        let device_name = device.name().ok();
        println!("[INFO] 使用音频输入设备: {}", device_name.as_deref().unwrap_or("Unknown"));
        self.device_name = device_name;

        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: 16000,
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer = Arc::clone(&self.buffer);
        let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

        let stream = device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let mut buf = buffer.lock().unwrap();
                for &sample in data {
                    buf.extend_from_slice(&sample.to_ne_bytes());
                }
            },
            err_fn,
            None,
        )?;

        stream.play()?;
        self.stream = Some(stream);
        self.is_capturing = true;
        self.start_time = Some(Instant::now());
        let mut buf = self.buffer.lock().unwrap();
        buf.clear();
        // 预分配 60 秒容量：16000 * 2 * 60 = 1,920,000 字节
        buf.reserve(1920000);

        Ok(())
    }

    async fn stop_capture(&mut self) -> Result<Vec<u8>> {
        if let Some(stream) = self.stream.take() {
            stream.pause()?;
        }
        self.is_capturing = false;
        self.start_time = None;
        let data = std::mem::take(&mut *self.buffer.lock().unwrap());
        Ok(data)
    }

    fn is_capturing(&self) -> bool {
        self.is_capturing
    }

    fn elapsed_seconds(&self) -> u64 {
        self.start_time.map(|t| t.elapsed().as_secs()).unwrap_or(0)
    }

    fn device_name(&self) -> Option<String> {
        self.device_name.clone()
    }
}

/// PCM 音频转 WAV 格式
/// 
/// 生成标准的 RIFF/WAVE 文件头 + PCM 数据
pub fn pcm_to_wav(pcm_data: &[u8], sample_rate: u32, channels: u16) -> Vec<u8> {
    let data_len = pcm_data.len() as u32;
    let byte_rate = sample_rate * channels as u32 * 2; // 16bit = 2 bytes
    let block_align = channels * 2;
    let total_len = 36 + data_len; // 文件总大小（不含 RIFF 头本身）

    let mut wav = Vec::with_capacity(44 + pcm_data.len());

    // RIFF chunk
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&total_len.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());           // Subchunk1Size (16 for PCM)
    wav.extend_from_slice(&1u16.to_le_bytes());            // AudioFormat (1 = PCM)
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());            // BitsPerSample

    // data sub-chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(pcm_data);

    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_start_stop_lifecycle() {
        let mut mock = MockAudioCapture::new();
        mock.expect_start_capture()
            .times(1)
            .returning(|| Ok(()));
        mock.expect_stop_capture()
            .times(1)
            .returning(|| Ok(vec![1u8, 2u8, 3u8]));
        
        mock.start_capture().await.unwrap();
        let data = mock.stop_capture().await.unwrap();
        assert_eq!(data, vec![1u8, 2u8, 3u8]);
    }

    #[tokio::test]
    async fn test_mock_is_capturing_state() {
        let mut mock = MockAudioCapture::new();
        mock.expect_is_capturing()
            .times(1)
            .returning(|| false);
        
        assert!(!mock.is_capturing());
    }

    #[tokio::test]
    async fn test_mock_elapsed_seconds() {
        let mut mock = MockAudioCapture::new();
        mock.expect_elapsed_seconds()
            .times(1)
            .returning(|| 42);
        
        assert_eq!(mock.elapsed_seconds(), 42);
    }

    #[tokio::test]
    async fn test_mock_stop_returns_empty_data() {
        let mut mock = MockAudioCapture::new();
        mock.expect_stop_capture()
            .times(1)
            .returning(|| Ok(Vec::new()));
        
        let data = mock.stop_capture().await.unwrap();
        assert!(data.is_empty());
    }

    #[tokio::test]
    async fn test_mock_start_capture_error() {
        let mut mock = MockAudioCapture::new();
        mock.expect_start_capture()
            .times(1)
            .returning(|| Err(anyhow::anyhow!("device unavailable")));
        
        let result = mock.start_capture().await;
        assert!(result.is_err());
    }

    #[test]
    fn test_pcm_to_wav_format() {
        let pcm = vec![0u8; 32000]; // 1 second @ 16kHz 16bit mono
        let wav = pcm_to_wav(&pcm, 16000, 1);

        // 检查 RIFF 头
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        
        // 检查 fmt 子块
        assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1); // PCM format
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1); // 1 channel
        assert_eq!(u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]), 16000);
        
        // 检查 data 子块
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 32000);
        
        // 检查总大小
        assert_eq!(wav.len(), 44 + 32000);
    }
}