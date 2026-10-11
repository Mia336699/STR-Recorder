//! Shared facade: status lookup dispatches by identity; recording downloads bind to a platform.
use super::{
    chaturbate::ChaturbateApi,
    identity::CB_PREFIX,
    stripchat::{StreamInfo, StripchatApi},
};
use crate::core::error::Result;
use std::{collections::HashMap, sync::Arc};

pub struct StreamApi {
    stripchat: StripchatApi,
    chaturbate: ChaturbateApi,
    cb_downloads: bool,
    channel: String,
}
impl StreamApi {
    pub fn new(
        api_proxy: Option<&str>,
        cdn_proxy: Option<&str>,
        mirror: Option<&str>,
        cache: Arc<parking_lot::Mutex<HashMap<String, String>>>,
    ) -> Result<Self> {
        Ok(Self {
            stripchat: StripchatApi::new(api_proxy, cdn_proxy, mirror, cache)?,
            chaturbate: ChaturbateApi::new(api_proxy, cdn_proxy)?,
            cb_downloads: false,
            channel: String::new(),
        })
    }
    pub fn new_api_only(
        api_proxy: Option<&str>,
        cdn_proxy: Option<&str>,
        mirror: Option<&str>,
    ) -> Result<Self> {
        Self::new(
            api_proxy,
            cdn_proxy,
            mirror,
            Arc::new(parking_lot::Mutex::new(HashMap::new())),
        )
    }
    pub fn with_channel(mut self, identity: &str) -> Self {
        self.cb_downloads = identity.starts_with(CB_PREFIX);
        self.channel = identity.to_owned();
        self
    }
    pub fn with_mouflon_keys(mut self, keys: HashMap<String, String>) -> Self {
        self.stripchat = self.stripchat.with_mouflon_keys(keys);
        self
    }
    pub fn with_resolution_selection(mut self, resolution: u32, preference: &str) -> Self {
        self.stripchat = self
            .stripchat
            .with_resolution_selection(resolution, preference);
        self
    }
    pub fn mouflon_keys(&self) -> &HashMap<String, String> {
        self.stripchat.mouflon_keys()
    }
    pub fn preferred_resolution(&self) -> u32 {
        self.stripchat.preferred_resolution()
    }
    pub fn prefers_higher_resolution(&self) -> bool {
        self.stripchat.prefers_higher_resolution()
    }
    pub async fn get_stream_info(&self, identity: &str, fetch: bool) -> Result<StreamInfo> {
        let identity = super::identity::normalize(identity)?;
        if let Some(name) = identity.strip_prefix(CB_PREFIX) {
            self.chaturbate
                .get_stream_info(
                    name,
                    fetch,
                    self.preferred_resolution(),
                    self.prefers_higher_resolution(),
                )
                .await
        } else {
            self.stripchat.get_stream_info(&identity, fetch).await
        }
    }
    pub fn with_audio_playlist(self, video: &str, audio: Option<String>) -> Self {
        self.chaturbate.cache_audio_playlist(video, audio);
        self
    }
    pub async fn prefetch_audio(&self, video: &str) -> Result<()> {
        if self.cb_downloads {
            self.chaturbate.prefetch_audio(video).await?;
        }
        Ok(())
    }
    pub async fn prepare_av_segment(
        &self,
        video_url: &str,
        sequence: u32,
        video: Vec<u8>,
    ) -> Result<Vec<u8>> {
        if let Some(name) = self.channel.strip_prefix(CB_PREFIX) {
            self.chaturbate
                .prepare_av_segment(
                    name,
                    video_url,
                    sequence,
                    video,
                    self.preferred_resolution(),
                    self.prefers_higher_resolution(),
                )
                .await
        } else {
            Ok(video)
        }
    }
    pub async fn fetch_playlist(&self, url: &str) -> Result<String> {
        if self.cb_downloads {
            self.chaturbate.fetch_playlist(url).await
        } else {
            self.stripchat.fetch_playlist(url).await
        }
    }
    pub async fn download_segment(&self, url: &str) -> Result<Vec<u8>> {
        if self.cb_downloads {
            self.chaturbate.download_segment(url).await
        } else {
            self.stripchat.download_segment(url).await
        }
    }
}
