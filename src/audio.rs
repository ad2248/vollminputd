use anyhow::{Context, Result};
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
    device_selector: Option<String>,
}

impl CpalAudioCapture {
    pub fn new(device_selector: Option<String>) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            is_capturing: false,
            start_time: None,
            stream: None,
            device_name: None,
            device_selector,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeviceInfo {
    pub id: Option<String>,
    pub name: String,
    pub is_default: bool,
}

pub fn list_input_devices() -> Result<Vec<InputDeviceInfo>> {
    let host = cpal::default_host();
    Ok(input_device_records(&host)?
        .into_iter()
        .map(|(_, info)| info)
        .collect())
}

fn input_device_records(host: &cpal::Host) -> Result<Vec<(cpal::Device, InputDeviceInfo)>> {
    let default_id = host
        .default_input_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.to_string());
    let devices = host.input_devices().context("无法枚举音频输入设备")?;

    Ok(devices
        .map(|device| {
            let id = device.id().ok().map(|id| id.to_string());
            let name = device
                .description()
                .ok()
                .map(|description| description.name().to_string())
                .or_else(|| id.clone())
                .unwrap_or_else(|| "未知设备".to_string());
            let is_default = id.is_some() && id == default_id;
            (device, InputDeviceInfo { id, name, is_default })
        })
        .collect())
}

fn matching_device_index(selector: &str, devices: &[InputDeviceInfo]) -> Result<usize> {
    if let Some(index) = devices
        .iter()
        .position(|device| device.id.as_deref() == Some(selector))
    {
        return Ok(index);
    }

    let name_matches: Vec<_> = devices
        .iter()
        .enumerate()
        .filter(|(_, device)| device.name == selector)
        .map(|(index, _)| index)
        .collect();

    match name_matches.as_slice() {
        [index] => Ok(*index),
        [] => anyhow::bail!("找不到指定的音频输入设备: {selector}"),
        _ => anyhow::bail!("存在多个名为 '{selector}' 的输入设备，请改用设备 ID"),
    }
}

fn resolve_input_device(host: &cpal::Host, selector: Option<&str>) -> Result<cpal::Device> {
    let Some(selector) = selector else {
        return host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("没有可用的默认音频输入设备"));
    };

    let mut records = input_device_records(host)?;
    let infos: Vec<_> = records.iter().map(|(_, info)| info.clone()).collect();
    let index = matching_device_index(selector, &infos).map_err(|error| {
        let available = infos
            .iter()
            .map(|device| match &device.id {
                Some(id) => format!("{} (ID: {})", device.name, id),
                None => device.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::anyhow!(
            "{error}。可用设备: {}",
            if available.is_empty() { "无" } else { &available }
        )
    })?;
    Ok(records.swap_remove(index).0)
}

#[async_trait::async_trait]
impl AudioCapture for CpalAudioCapture {
    async fn start_capture(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = resolve_input_device(&host, self.device_selector.as_deref())?;

        // cpal 0.18: device.name() 被 device.description() / device.id() 替代
        // DeviceId 实现了 Display（不再是 name()），直接 format
        let device_name = device
            .description()
            .ok()
            .map(|desc| desc.name().to_string())
            .or_else(|| device.id().ok().map(|id| id.to_string()));
        println!("[INFO] 使用音频输入设备: {}", device_name.as_deref().unwrap_or("Unknown"));
        self.device_name = device_name;

        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: 16000,
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer = Arc::clone(&self.buffer);
        let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

        // cpal 0.18: build_*_stream 现在收 StreamConfig by value（StreamConfig: Copy）
        let stream = device.build_input_stream(
            config,
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

    fn device(id: Option<&str>, name: &str) -> InputDeviceInfo {
        InputDeviceInfo {
            id: id.map(str::to_string),
            name: name.to_string(),
            is_default: false,
        }
    }

    #[test]
    fn device_id_match_takes_priority_over_name() {
        let devices = vec![
            device(Some("first-id"), "target"),
            device(Some("target"), "other"),
        ];

        assert_eq!(matching_device_index("target", &devices).unwrap(), 1);
    }

    #[test]
    fn unique_exact_device_name_can_be_selected() {
        let devices = vec![device(Some("first-id"), "USB Microphone")];

        assert_eq!(matching_device_index("USB Microphone", &devices).unwrap(), 0);
    }

    #[test]
    fn duplicate_device_name_requires_id() {
        let devices = vec![
            device(Some("first-id"), "USB Microphone"),
            device(Some("second-id"), "USB Microphone"),
        ];

        let error = matching_device_index("USB Microphone", &devices).unwrap_err();
        assert!(error.to_string().contains("设备 ID"));
    }

    #[test]
    fn unknown_device_is_rejected() {
        let devices = vec![device(Some("first-id"), "USB Microphone")];

        let error = matching_device_index("missing", &devices).unwrap_err();
        assert!(error.to_string().contains("找不到"));
    }

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
