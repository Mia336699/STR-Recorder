//! HLS 播放列表解析与 Mouflon 解密 / HLS Playlist Parsing and Mouflon Decryption
//!
//! 解析 Stripchat 的 HLS m3u8 播放列表，提取分片 URL 和 fMP4 初始化段 URL。
//! 支持 Mouflon 加密系统：通过 SHA-256 密钥对分片 URL 进行 XOR 解密。
//!
//! Parses Stripchat's HLS m3u8 playlists, extracting segment URLs and fMP4 init segment URLs.
//! Supports the Mouflon encryption system: XOR-decrypts segment URLs using SHA-256 keys.

use crate::core::error::{AppError, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::LazyLock;

/// 用于从加密 URL 中提取加密字符串和序号的正则表达式。
/// Regex for extracting the encrypted string and sequence number from an encrypted URL.
static SEGMENT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_([^_]+)_(\d+(?:_part\d+)?)\.mp4(?:[?#].*)?").unwrap());

/// HLS 分片信息 / HLS segment information
#[derive(Debug, Clone)]
pub struct HlsSegment {
    /// 分片的完整 URL（已解密）/ Full segment URL (decrypted)
    pub url: String,
    /// 分片序号（用于去重）/ Segment sequence number (for deduplication)
    pub sequence: u32,
    /// Media duration declared by EXTINF, in seconds.
    pub duration_secs: f64,
}

/// 解析 HLS m3u8 播放列表，返回分片列表和 fMP4 初始化段 URL。
/// Parse an HLS m3u8 playlist, returning the segment list and fMP4 init segment URL.
///
/// # 参数 / Parameters
/// - `playlist`: m3u8 文本内容 / m3u8 text content
/// - `url_prefix`: 用于将相对路径转为绝对 URL 的前缀 / Prefix for converting relative paths to absolute URLs
/// - `mouflon_keys`: Mouflon 解密密钥表（pkey -> pdkey）/ Mouflon decryption key map (pkey -> pdkey)
///
/// # 返回值 / Returns
/// `(segments, init_url)` 元组 / Tuple of `(segments, init_url)`
pub fn parse_playlist(
    playlist: &str,
    url_prefix: &str,
    mouflon_keys: &HashMap<String, String>,
) -> Result<(Vec<HlsSegment>, Option<String>)> {
    if !playlist.trim_start().starts_with("#EXTM3U") {
        return Err(AppError::Other("Invalid HLS media playlist".into()));
    }
    let base = reqwest::Url::parse(&format!("{}/", url_prefix.trim_end_matches('/')))
        .map_err(|e| AppError::Other(format!("Invalid HLS base URL: {e}")))?;
    let resolve = |path: &str| -> Result<String> {
        let url = base
            .join(path)
            .map_err(|e| AppError::Other(e.to_string()))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(AppError::Other("Unsupported HLS URL scheme".into()));
        }
        Ok(url.into())
    };
    let media_sequence = playlist
        .lines()
        .find_map(|line| line.trim().strip_prefix("#EXT-X-MEDIA-SEQUENCE:"))
        .map(|value| {
            value
                .parse::<u32>()
                .map_err(|e| AppError::Other(format!("Invalid HLS sequence: {e}")))
        })
        .transpose()?;
    let mut segments = Vec::new();
    let mut mp4_header_url = None;
    let mut current_pkey: Option<&str> = None;
    let mut duration_secs = 0.0;

    let lines: Vec<&str> = playlist.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let line = line.trim();
        if line.starts_with("#EXT-X-KEY:")
            && !line.split(',').any(|attr| {
                attr.trim()
                    .strip_prefix("#EXT-X-KEY:")
                    .unwrap_or(attr.trim())
                    == "METHOD=NONE"
            })
        {
            return Err(AppError::Other(
                "Encrypted HLS media is not supported".into(),
            ));
        }
        if line.starts_with("#EXT-X-BYTERANGE:")
            || (line.starts_with("#EXT-X-MAP:") && line.contains("BYTERANGE="))
        {
            return Err(AppError::Other("Byte-range HLS is not supported".into()));
        }
        if let Some(value) = line.strip_prefix("#EXTINF:") {
            duration_secs = value
                .split(',')
                .next()
                .unwrap_or("")
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(0.0);
        }
        // 解析 Mouflon 加密标签，获取当前 pkey 对应的解密密钥
        // Parse Mouflon encryption tag to get the decryption key for the current pkey
        if line.contains("#EXT-X-MOUFLON:PSCH") {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 4 {
                let pkey = parts[3];
                current_pkey = mouflon_keys.get(pkey).map(|s| s.as_str());
            }
        }

        // 解析 fMP4 初始化段 URL（EXT-X-MAP）
        // Parse fMP4 init segment URL (EXT-X-MAP)
        if line.contains("EXT-X-MAP:URI")
            && let Some(start) = line.find('"')
            && let Some(end) = line[start + 1..].find('"')
        {
            let header_path = &line[start + 1..start + 1 + end];
            mp4_header_url = Some(resolve(header_path)?);
        }

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // 检查前一行是否为 Mouflon URI 标签（加密分片的实际 URL 在标签中）
        // Check if the previous line is a Mouflon URI tag (actual URL for encrypted segments is in the tag)
        let mouflon_uri_line = if i > 0 && lines[i - 1].starts_with("#EXT-X-MOUFLON:URI:") {
            Some(lines[i - 1])
        } else {
            None
        };

        let url = if let Some(mouflon_line) = mouflon_uri_line {
            let raw_url = mouflon_line.trim_start_matches("#EXT-X-MOUFLON:URI:");
            let encoded_url = if raw_url.starts_with("https://") {
                raw_url.to_string()
            } else if raw_url.starts_with("//") {
                format!("https:{}", raw_url)
            } else {
                format!("https://{}", raw_url)
            };

            // 若有解密密钥则解密 URL，否则直接使用
            // Decrypt URL if key is available, otherwise use as-is
            if let Some(key) = current_pkey {
                decrypt_segment_url(&encoded_url, key).unwrap_or(encoded_url)
            } else {
                encoded_url
            }
        } else {
            resolve(line)?
        };

        // Standard sequence numbers work even when CDN filenames are hashes or timestamps.
        // Preserve Stripchat's Mouflon filename sequence convention.
        let sequence = if mouflon_uri_line.is_some() {
            extract_sequence(&url).unwrap_or(segments.len() as u32)
        } else if let Some(first) = media_sequence {
            first
                .checked_add(segments.len() as u32)
                .ok_or_else(|| AppError::Other("HLS sequence overflow".into()))?
        } else {
            extract_sequence(&url).unwrap_or(segments.len() as u32)
        };
        segments.push(HlsSegment {
            url,
            sequence,
            duration_secs,
        });
        duration_secs = 0.0;
    }

    Ok((segments, mp4_header_url))
}

/// 从完整 URL 中提取 URL 前缀（去掉最后一个路径段）。
/// Extract the URL prefix from a full URL (removes the last path segment).
pub fn get_url_prefix(url: &str) -> String {
    let parts: Vec<&str> = url.split('/').collect();
    if parts.len() > 1 {
        parts[..parts.len() - 1].join("/")
    } else {
        url.to_string()
    }
}

/// 使用 SHA-256 密钥对 Mouflon 加密的分片 URL 进行 XOR 解密。
/// Decrypt a Mouflon-encrypted segment URL using XOR with a SHA-256 key.
///
/// 解密流程：提取加密字符串 → Base64 解码（反转后补齐）→ SHA-256(key) XOR 解密 → 替换回 URL
/// Decryption flow: extract encrypted string → Base64 decode (reversed + padded) → SHA-256(key) XOR decrypt → replace in URL
fn decrypt_segment_url(encoded_url: &str, key: &str) -> Result<String> {
    let captures = SEGMENT_REGEX
        .captures(encoded_url)
        .ok_or_else(|| AppError::Other("Cannot parse encrypted URL".to_string()))?;

    let encrypted_str = captures.get(1).unwrap().as_str();

    // 反转字符串并补齐 Base64 填充 / Reverse string and pad for Base64
    let mut reversed: String = encrypted_str.chars().rev().collect();
    while !reversed.len().is_multiple_of(4) {
        reversed.push('=');
    }

    let encrypted_bytes = STANDARD
        .decode(&reversed)
        .map_err(|e| AppError::Other(format!("Base64 decode error: {}", e)))?;

    // 使用 SHA-256(key) 作为 XOR 密钥流 / Use SHA-256(key) as XOR keystream
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let key_bytes = hasher.finalize();

    let decrypted: Vec<u8> = encrypted_bytes
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ key_bytes[i % key_bytes.len()])
        .collect();

    let decrypted_str = String::from_utf8_lossy(&decrypted);
    Ok(encoded_url.replace(encrypted_str, &decrypted_str))
}

/// 从分片 URL 的文件名中提取序号（最后一个 `_` 后、`.` 前的数字）。
/// Extract the sequence number from a segment URL's filename (number after the last `_`, before `.`).
fn extract_sequence(url: &str) -> Option<u32> {
    let filename = url.split('/').next_back()?;
    let parts: Vec<&str> = filename.split('_').collect();
    let last = parts.last()?;
    let num_str = last.split('.').next()?;
    num_str.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sliding_windows_use_media_sequence_and_resolve_paths() {
        let text = "#EXTM3U\n#EXT-X-MEDIA-SEQUENCE:105\n#EXT-X-MAP:URI=\"../init.mp4?t=x\"\n#EXTINF:2,\n../hash.ts?t=1\n#EXTINF:3,\n//cdn.test/other.ts\n";
        let (segments, init) =
            parse_playlist(text, "https://cdn.test/live/quality", &HashMap::new()).unwrap();
        assert_eq!(segments[0].sequence, 105);
        assert_eq!(segments[1].sequence, 106);
        assert_eq!(segments[0].url, "https://cdn.test/live/hash.ts?t=1");
        assert_eq!(segments[1].url, "https://cdn.test/other.ts");
        assert_eq!(init.unwrap(), "https://cdn.test/live/init.mp4?t=x");
        assert_eq!(segments[1].duration_secs, 3.0);
        let next = text.replace("SEQUENCE:105", "SEQUENCE:106");
        assert_eq!(
            parse_playlist(&next, "https://cdn.test/live/quality", &HashMap::new())
                .unwrap()
                .0[0]
                .sequence,
            106
        );
    }
    #[test]
    fn rejects_invalid_encrypted_and_range_media() {
        for text in [
            "<html>Denied</html>",
            "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key\"\n1.ts",
            "#EXTM3U\n#EXT-X-BYTERANGE:200@0\n1.ts",
        ] {
            assert!(parse_playlist(text, "https://cdn.test/live", &HashMap::new()).is_err());
        }
        assert!(
            parse_playlist(
                "#EXTM3U\n#EXT-X-KEY:METHOD=NONE\n1.ts",
                "https://cdn.test/live",
                &HashMap::new()
            )
            .is_ok()
        );
    }
    #[test]
    fn keeps_stripchat_mouflon_sequences() {
        let text = "#EXTM3U\n#EXT-X-MEDIA-SEQUENCE:1\n#EXTINF:2,\n#EXT-X-MOUFLON:URI:https://cdn.test/stream_x_456.mp4\nplaceholder.mp4";
        assert_eq!(
            parse_playlist(text, "https://cdn.test/live", &HashMap::new())
                .unwrap()
                .0[0]
                .sequence,
            456
        );
    }
}
