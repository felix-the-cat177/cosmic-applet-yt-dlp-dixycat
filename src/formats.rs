// SPDX-License-Identifier: GPL-3.0-only

use std::fmt::Display;

// Re-export the upstream types used by `From` impls so that any path
// adjustments only need to happen here if the crate reshuffles modules.
use yt_dlp::model::selector::{
    AudioCodecPreference as YtAudioCodec, AudioQuality as YtAudioQuality,
    VideoCodecPreference as YtVideoCodec, VideoQuality as YtVideoQuality,
};

// ---------------------------------------------------------------------------
// Video container format (MP4, MKV, WebM)
// ---------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum VideoContainer {
    #[default]
    MP4,
    MKV,
    WebM,
}

impl VideoContainer {
    pub fn extension(&self) -> &'static str {
        match self {
            VideoContainer::MP4 => "mp4",
            VideoContainer::MKV => "mkv",
            VideoContainer::WebM => "webm",
        }
    }
}

impl Display for VideoContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoContainer::MP4 => write!(f, "MP4"),
            VideoContainer::MKV => write!(f, "MKV"),
            VideoContainer::WebM => write!(f, "WebM"),
        }
    }
}

// ---------------------------------------------------------------------------
// Video quality (UI-facing labels)
// ---------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum VideoQuality {
    #[default]
    Highest,
    FHD,
    HD,
    SD,
    Lowest,
}

impl Display for VideoQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoQuality::Highest => write!(f, "Highest"),
            VideoQuality::FHD => write!(f, "1080p"),
            VideoQuality::HD => write!(f, "720p"),
            VideoQuality::SD => write!(f, "480p"),
            VideoQuality::Lowest => write!(f, "Lowest"),
        }
    }
}

impl From<VideoQuality> for YtVideoQuality {
    fn from(val: VideoQuality) -> Self {
        match val {
            VideoQuality::Highest => YtVideoQuality::Best,
            VideoQuality::FHD => YtVideoQuality::High,
            VideoQuality::HD => YtVideoQuality::Medium,
            VideoQuality::SD => YtVideoQuality::Low,
            VideoQuality::Lowest => YtVideoQuality::Worst,
        }
    }
}

// ---------------------------------------------------------------------------
// Video codec (UI-facing labels)
// ---------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum VideoCodec {
    AV1,
    AVC1,
    VP9,
    #[default]
    Any,
}

impl Display for VideoCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoCodec::AV1 => write!(f, "AV1"),
            VideoCodec::AVC1 => write!(f, "AVC1"),
            VideoCodec::VP9 => write!(f, "VP9"),
            VideoCodec::Any => write!(f, "Any"),
        }
    }
}

impl From<VideoCodec> for YtVideoCodec {
    fn from(val: VideoCodec) -> Self {
        match val {
            VideoCodec::AV1 => YtVideoCodec::AV1,
            VideoCodec::AVC1 => YtVideoCodec::AVC1,
            VideoCodec::VP9 => YtVideoCodec::VP9,
            VideoCodec::Any => YtVideoCodec::Any,
        }
    }
}

// ---------------------------------------------------------------------------
// Audio quality (UI-facing labels)
// ---------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum AudioQuality {
    #[default]
    Best,
    High,
    Medium,
    Low,
    Worst,
}

impl Display for AudioQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioQuality::Best => write!(f, "Highest"),
            AudioQuality::High => write!(f, "192kbps"),
            AudioQuality::Medium => write!(f, "128kbps"),
            AudioQuality::Low => write!(f, "96kbps"),
            AudioQuality::Worst => write!(f, "Lowest"),
        }
    }
}

impl From<AudioQuality> for YtAudioQuality {
    fn from(val: AudioQuality) -> Self {
        match val {
            AudioQuality::Best => YtAudioQuality::Best,
            AudioQuality::High => YtAudioQuality::High,
            AudioQuality::Medium => YtAudioQuality::Medium,
            AudioQuality::Low => YtAudioQuality::Low,
            AudioQuality::Worst => YtAudioQuality::Worst,
        }
    }
}

// ---------------------------------------------------------------------------
// Audio codec (UI-facing labels)
// ---------------------------------------------------------------------------

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum AudioCodec {
    MP3,
    AAC,
    Opus,
    FLAC,
    WAV,
    #[default]
    Any,
}

impl AudioCodec {
    /// Returns the recommended file extension for this audio codec.
    pub fn extension(&self) -> &'static str {
        match self {
            AudioCodec::MP3 => "mp3",
            AudioCodec::AAC => "m4a",
            AudioCodec::Opus => "opus",
            AudioCodec::FLAC => "flac",
            AudioCodec::WAV => "wav",
            AudioCodec::Any => "m4a",
        }
    }
}

impl Display for AudioCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioCodec::MP3 => write!(f, "MP3"),
            AudioCodec::AAC => write!(f, "AAC (M4A)"),
            AudioCodec::Opus => write!(f, "Opus"),
            AudioCodec::FLAC => write!(f, "FLAC"),
            AudioCodec::WAV => write!(f, "WAV"),
            AudioCodec::Any => write!(f, "Any"),
        }
    }
}

impl From<AudioCodec> for YtAudioCodec {
    fn from(val: AudioCodec) -> Self {
        match val {
            AudioCodec::MP3 => YtAudioCodec::MP3,
            AudioCodec::AAC => YtAudioCodec::AAC,
            AudioCodec::Opus => YtAudioCodec::Opus,
            AudioCodec::FLAC => YtAudioCodec::Any,
            AudioCodec::WAV => YtAudioCodec::Any,
            AudioCodec::Any => YtAudioCodec::Any,
        }
    }
}
