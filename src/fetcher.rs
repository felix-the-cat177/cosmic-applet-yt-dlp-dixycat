// SPDX-License-Identifier: GPL-3.0-only

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use yt_dlp::{
    client::deps::{Libraries, LibraryInstaller},
    Downloader,
};

use cosmic::Application;
use crate::applet::Ytdlp;

/// Install and cache yt-dlp and ffmpeg binaries, returning the deps directory.
pub async fn binaries() -> PathBuf {
    let deps_dir = xdg::BaseDirectories::with_prefix(Ytdlp::APP_ID)
        .expect("Failed to get xdg base dirs")
        .get_data_home();

    let installer = LibraryInstaller::new(deps_dir.clone());
    let youtube_path = deps_dir.join("yt-dlp");
    let ffmpeg_path = deps_dir.join("ffmpeg");

    if !youtube_path.exists() {
        installer
            .install_youtube(None)
            .await
            .expect("Failed to download yt-dlp");
    }

    if !ffmpeg_path.exists() {
        installer
            .install_ffmpeg(None)
            .await
            .expect("Failed to download ffmpeg");
    }

    deps_dir
}

/// Create a [`Downloader`] instance targeting the given output directory.
pub async fn with_output_dir(lib_dir: &Path, output_dir: PathBuf) -> Downloader {
    let libraries = Libraries::new(lib_dir.join("yt-dlp"), lib_dir.join("ffmpeg"));

    let mut dl = Downloader::builder(libraries, output_dir)
        .with_timeout(Duration::from_secs(300))
        .build()
        .await
        .expect("Failed to create downloader");

    // Download up to 4 DASH/HLS fragments in parallel — significantly faster
    // for YouTube and other adaptive-stream platforms.
    dl.add_arg("--concurrent-fragments");
    dl.add_arg("4");

    dl
}
