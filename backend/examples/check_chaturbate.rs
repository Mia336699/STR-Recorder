//! Read-only diagnostic: validates public status, selected HLS and media parsing.
use stripchat_recorder_lib::{streaming::{identity, platform::StreamApi}, recording::hls::{get_url_prefix, parse_playlist}};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args().nth(1).ok_or("Supply a Chaturbate room URL or chaturbate~username")?;
    let key = identity::normalize(&input)?;
    if !key.starts_with(identity::CB_PREFIX) { return Err("Chaturbate room required".into()); }
    let api = StreamApi::new_api_only(None, None, None)?.with_channel(&key);
    let info = api.get_stream_info(&key, true).await?;
    println!("online={} recordable={} status={}", info.is_online, info.is_recordable, info.status);
    if let Some(url) = info.playlist_url {
        let media = api.fetch_playlist(&url).await?;
        let (segments, init) = parse_playlist(&media, &get_url_prefix(&url), api.mouflon_keys())?;
        println!("HLS verified: {} segments, init_segment={}", segments.len(), init.is_some());
    }
    Ok(())
}
