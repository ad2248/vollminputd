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
}

pub struct CpalAudioCapture {
    buffer: Arc<Mutex<Vec<u8>>>,
    is_capturing: bool,
    start_time: Option<Instant>,
    stream: Option<cpal::Stream>,
}

impl CpalAudioCapture {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            is_capturing: false,
            start_time: None,
            stream: None,
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

        println!("[INFO] 使用音频输入设备: {}", device.name().unwrap_or_else(|_| "Unknown".to_string()));

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
        self.buffer.lock().unwrap().clear();

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
}
