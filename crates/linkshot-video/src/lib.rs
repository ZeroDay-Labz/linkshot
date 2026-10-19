//! Video support for linkshot: the lossless recording spool, animated-GIF encoding, MP4 export and
//! import through `ffmpeg`, and a background player.
//!
//! Every video the app touches is turned into a *spool* (`.lsv`, see [`spool`]): the recorder writes
//! one while capturing, opened GIF/MP4 files are decoded into one in the cache, and the GIF/MP4
//! exporters and the player read frames from it through the [`FrameSource`] trait. Keeping the
//! master lossless means a recording saved as GIF can later be exported to MP4 at full quality.

pub mod ffmpeg;
pub mod gif;
pub mod import;
pub mod player;
pub mod quant;
pub mod source;
pub mod spool;

pub use gif::{export_gif, GifOptions, GifReport};
pub use ffmpeg::{export_mp4, Mp4Options, Mp4Report};
pub use player::{Player, PlayerCmd, PlayerEvent, PlayerFrame};
pub use source::{FrameSource, MemorySource};
pub use spool::{SpoolReader, SpoolWriter};

/// File extensions the app treats as videos (opened through [`import::open_video`]).
pub const VIDEO_EXTENSIONS: &[&str] = &["gif", "mp4", "mov", "webm", "mkv", "lsv"];

pub fn is_video_extension(ext: &str) -> bool {
    let e = ext.to_ascii_lowercase();
    VIDEO_EXTENSIONS.contains(&e.as_str())
}

/// `mm:ss.d` style duration label used by the player and the tray badge.
pub fn format_duration(ms: u32) -> String {
    let total = ms / 1000;
    let tenths = (ms % 1000) / 100;
    if total >= 3600 { format!("{}:{:02}:{:02}", total / 3600, (total / 60) % 60, total % 60) } else { format!("{}:{:02}.{}", total / 60, total % 60, tenths) }
}
