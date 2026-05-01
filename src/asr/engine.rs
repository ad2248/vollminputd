use anyhow::Result;

#[mockall::automock]
#[async_trait::async_trait]
pub trait AsrEngine: Send + Sync {
    async fn recognize(&self, audio_data: &[u8]) -> Result<String>;
}