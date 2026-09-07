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

/// Search for a binary in standard system locations or PATH.
fn find_system_binary(name: &str) -> Option<PathBuf> {
    for common_dir in ["/usr/bin", "/usr/local/bin", "/bin"] {
        let p = Path::new(common_dir).join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Verify if a binary can be executed successfully with a quick test argument.
async fn is_binary_valid(path: &Path, test_arg: &str) -> bool {
    if !path.is_file() {
        return false;
    }
    tokio::process::Command::new(path)
        .arg(test_arg)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Install and cache yt-dlp, ffmpeg and ffprobe binaries, returning the deps directory.
pub async fn binaries() -> PathBuf {
    let deps_dir = xdg::BaseDirectories::with_prefix(Ytdlp::APP_ID)
        .expect("Failed to get xdg base dirs")
        .get_data_home();

    let legacy_dir = xdg::BaseDirectories::with_prefix("dev.DBrox.CosmicYtdlp")
        .map(|b| b.get_data_home())
        .unwrap_or_default();

    let youtube_path = deps_dir.join("yt-dlp");
    let ffmpeg_path = deps_dir.join("ffmpeg");
    let ffprobe_path = deps_dir.join("ffprobe");

    let _ = tokio::fs::create_dir_all(&deps_dir).await;

    // 1. Setup FFmpeg & FFprobe
    // Prefer system binaries (e.g. from distro package 'ffmpeg')
    if let Some(sys_ffmpeg) = find_system_binary("ffmpeg") {
        if !ffmpeg_path.exists() {
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(&sys_ffmpeg, &ffmpeg_path);
            #[cfg(not(unix))]
            let _ = tokio::fs::copy(&sys_ffmpeg, &ffmpeg_path).await;
        }
    }
    if let Some(sys_ffprobe) = find_system_binary("ffprobe") {
        if !ffprobe_path.exists() {
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(&sys_ffprobe, &ffprobe_path);
            #[cfg(not(unix))]
            let _ = tokio::fs::copy(&sys_ffprobe, &ffprobe_path).await;
        }
    }

    // Check legacy directory if still not present
    if !ffmpeg_path.exists() && legacy_dir.join("ffmpeg").exists() {
        let _ = tokio::fs::copy(legacy_dir.join("ffmpeg"), &ffmpeg_path).await;
    }

    // If still missing, download ffmpeg via installer without crashing on checksum mismatch
    if !ffmpeg_path.exists() {
        let installer = LibraryInstaller::new(deps_dir.clone());
        if let Err(e) = installer.install_ffmpeg(None).await {
            eprintln!("Warning: Failed to download ffmpeg: {e}");
            let _ = tokio::fs::remove_file(deps_dir.join("ffmpeg-release.zip")).await;
        }
    }

    // Ensure ffmpeg is executable
    #[cfg(unix)]
    if ffmpeg_path.exists() {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&ffmpeg_path) {
            let mut perms = meta.permissions();
            if perms.mode() & 0o111 == 0 {
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&ffmpeg_path, perms);
            }
        }
    }

    // 2. Setup yt-dlp
    // Copy cached binary from legacy directory if available
    if !youtube_path.exists() && legacy_dir.join("yt-dlp").exists() {
        let _ = tokio::fs::copy(legacy_dir.join("yt-dlp"), &youtube_path).await;
    }

    // Test if the existing yt-dlp binary actually works
    let yt_valid = is_binary_valid(&youtube_path, "--version").await;
    if !yt_valid {
        // Clean up partial or broken binaries
        let _ = tokio::fs::remove_file(&youtube_path).await;
        let _ = tokio::fs::remove_file(deps_dir.join("yt-dlp.parts")).await;

        let mut found_system = false;
        if let Some(sys_ytdlp) = find_system_binary("yt-dlp") {
            if is_binary_valid(&sys_ytdlp, "--version").await {
                #[cfg(unix)]
                let _ = std::os::unix::fs::symlink(&sys_ytdlp, &youtube_path);
                #[cfg(not(unix))]
                let _ = tokio::fs::copy(&sys_ytdlp, &youtube_path).await;
                found_system = true;
            }
        }

        if !found_system {
            // Download latest official release directly from yt-dlp GitHub
            let arch = std::env::consts::ARCH;
            let download_url = if arch == "aarch64" {
                "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64"
            } else {
                "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
            };

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_default();

            let mut downloaded = false;
            if let Ok(response) = client.get(download_url).send().await {
                if response.status().is_success() {
                    if let Ok(bytes) = response.bytes().await {
                        let tmp_target = deps_dir.join("yt-dlp.tmp");
                        if tokio::fs::write(&tmp_target, &bytes).await.is_ok() {
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let _ = std::fs::set_permissions(&tmp_target, std::fs::Permissions::from_mode(0o755));
                            }
                            if tokio::fs::rename(tmp_target, &youtube_path).await.is_ok() {
                                downloaded = true;
                            }
                        }
                    }
                }
            }

            // Fallback to LibraryInstaller if direct download didn't succeed
            if !downloaded && !youtube_path.exists() {
                let installer = LibraryInstaller::new(deps_dir.clone());
                let _ = installer.install_youtube(None).await;
            }
        }
    }

    // Ensure yt-dlp is executable
    #[cfg(unix)]
    if youtube_path.exists() {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&youtube_path) {
            let mut perms = meta.permissions();
            if perms.mode() & 0o111 == 0 {
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&youtube_path, perms);
            }
        }
    }

    // Always check if yt-dlp needs update by comparing with latest release
    if youtube_path.exists() {
        let ytdlp_bin = youtube_path.clone();
        tokio::spawn(async move {
            // Check for updates and apply if available
            let _ = tokio::process::Command::new(&ytdlp_bin)
                .args(["--update-to", "stable"])
                .output()
                .await;
        });
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
    dl.add_arg("--concurrent-fragments");
    dl.add_arg("4");

    dl
}
