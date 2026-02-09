use std::sync::Arc;

use anyhow::Result;

pub struct TunService {
    
}

impl TunService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
        })
    }

    pub async fn stop(&self) -> Result<()> {
        Ok(())
    }

    pub async fn serve(&self) -> Result<()> {
        Ok(())
    }
}