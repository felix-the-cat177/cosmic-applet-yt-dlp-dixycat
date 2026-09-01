// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;
use std::path::PathBuf;

use cosmic::app::{Core, Task};
use cosmic::applet::padded_control;
use cosmic::cosmic_theme::Spacing;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::destroy_popup;
use cosmic::iced::widget::{column, row};
use cosmic::iced::{window, Alignment, Length, Limits};
use cosmic::widget::segmented_button::{Entity, SingleSelectModel};
use cosmic::widget::text::body;
use cosmic::widget::{divider, segmented_control, text_input};
use cosmic::{Action, Application, Apply, Element};

use ashpd::desktop::file_chooser::SelectedFiles;
use notify_rust::Notification;

use crate::formats::{AudioCodec, AudioQuality, VideoCodec, VideoContainer, VideoQuality};
use crate::{fetcher, fl, fl_str};

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Parses yt-dlp CLI progress output line into (percent, speed_mbps, eta_secs, downloaded_bytes, total_bytes)
fn parse_ytdlp_progress_line(line: &str) -> Option<(f32, f64, Option<u64>, u64, u64)> {
    if !line.starts_with("[download]") {
        return None;
    }
    let rest = line.strip_prefix("[download]")?.trim();
    // Example: "45.2% of 12.34MiB at 2.45MiB/s ETA 00:03"
    let pct_idx = rest.find('%')?;
    let percent = rest[..pct_idx].trim().parse::<f32>().ok()?;

    let mut speed_mbps = 0.0;
    if let Some(at_idx) = rest.find(" at ") {
        let after_at = &rest[at_idx + 4..];
        let speed_token = after_at.split_whitespace().next().unwrap_or("");
        if speed_token.ends_with("MiB/s") || speed_token.ends_with("MB/s") {
            let num_str = speed_token.trim_end_matches("MiB/s").trim_end_matches("MB/s");
            speed_mbps = num_str.parse::<f64>().unwrap_or(0.0);
        } else if speed_token.ends_with("KiB/s") || speed_token.ends_with("KB/s") {
            let num_str = speed_token.trim_end_matches("KiB/s").trim_end_matches("KB/s");
            speed_mbps = num_str.parse::<f64>().unwrap_or(0.0) / 1024.0;
        } else if speed_token.ends_with("GiB/s") || speed_token.ends_with("GB/s") {
            let num_str = speed_token.trim_end_matches("GiB/s").trim_end_matches("GB/s");
            speed_mbps = num_str.parse::<f64>().unwrap_or(0.0) * 1024.0;
        }
    }

    let mut eta_secs = None;
    if let Some(eta_idx) = rest.find("ETA ") {
        let eta_token = rest[eta_idx + 4..].split_whitespace().next().unwrap_or("");
        let parts: Vec<&str> = eta_token.split(':').collect();
        if parts.len() == 2 {
            let mins = parts[0].parse::<u64>().unwrap_or(0);
            let secs = parts[1].parse::<u64>().unwrap_or(0);
            eta_secs = Some(mins * 60 + secs);
        } else if parts.len() == 3 {
            let hours = parts[0].parse::<u64>().unwrap_or(0);
            let mins = parts[1].parse::<u64>().unwrap_or(0);
            let secs = parts[2].parse::<u64>().unwrap_or(0);
            eta_secs = Some(hours * 3600 + mins * 60 + secs);
        }
    }

    let mut total_bytes = 0u64;
    if let Some(of_idx) = rest.find(" of ") {
        let after_of = &rest[of_idx + 4..];
        let total_token = after_of.trim_start_matches('~').trim().split_whitespace().next().unwrap_or("");
        if total_token.ends_with("MiB") || total_token.ends_with("MB") {
            let num = total_token.trim_end_matches("MiB").trim_end_matches("MB").parse::<f64>().unwrap_or(0.0);
            total_bytes = (num * 1_048_576.0) as u64;
        } else if total_token.ends_with("KiB") || total_token.ends_with("KB") {
            let num = total_token.trim_end_matches("KiB").trim_end_matches("KB").parse::<f64>().unwrap_or(0.0);
            total_bytes = (num * 1024.0) as u64;
        } else if total_token.ends_with("GiB") || total_token.ends_with("GB") {
            let num = total_token.trim_end_matches("GiB").trim_end_matches("GB").parse::<f64>().unwrap_or(0.0);
            total_bytes = (num * 1_073_741_824.0) as u64;
        }
    }

    let downloaded_bytes = if total_bytes > 0 {
        ((percent / 100.0) * total_bytes as f32) as u64
    } else {
        0
    };

    Some((percent, speed_mbps, eta_secs, downloaded_bytes, total_bytes))
}

/// Cleans up empty (0-byte) files and leftover temporary files from `dir`.
async fn cleanup_zero_and_temp_files(dir: &std::path::Path) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if meta.is_file() {
            // Remove 0KB empty corrupted files or leftover temporary part files
            if meta.len() == 0 || name.ends_with(".part") || name.ends_with(".ytdl") || name.starts_with("temp_video_") || name.starts_with("temp_audio_") {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
    }
}

/// Runs a download through the unified yt-dlp native binary engine with live progress streaming.
#[allow(clippy::too_many_arguments)]
async fn run_download_job(
    download_id: u32,
    video_selected: bool,
    url: &str,
    custom_name: Option<String>,
    video_container_ext: &str,
    downloader: &yt_dlp::Downloader,
    output_dir_ref: &std::path::Path,
    audio_ext: &str,
    video_quality: VideoQuality,
    audio_quality: AudioQuality,
    output: &mut cosmic::iced::futures::channel::mpsc::Sender<cosmic::Action<Message>>,
    mut notify: notify_rust::Notification,
) {
    use cosmic::iced::futures::SinkExt;
    use tokio::io::AsyncBufReadExt;

    // Detect if this is a playlist URL
    let is_playlist = url.contains("list=") || url.contains("/playlist");

    // Extract title quickly for display and notifications
    let title_output = tokio::process::Command::new(&downloader.libraries().youtube)
        .arg("--print")
        .arg("%(title)s")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg(url)
        .output()
        .await;

    let extracted_title = title_output
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let display_title = custom_name.clone()
        .or(extracted_title)
        .unwrap_or_else(|| "Download".to_string());

    let out_template = if let Some(ref name) = custom_name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            format!("{}/{}.%(ext)s", output_dir_ref.display(), trimmed)
        } else {
            format!("{}/%(title)s.%(ext)s", output_dir_ref.display())
        }
    } else {
        format!("{}/%(title)s.%(ext)s", output_dir_ref.display())
    };

    let mut cmd = tokio::process::Command::new(&downloader.libraries().youtube);
    cmd.arg("--ffmpeg-location").arg(&downloader.libraries().ffmpeg);
    cmd.arg("--newline");
    cmd.arg("--progress");
    cmd.arg("--no-mtime"); // Set modification time to download time
    cmd.arg("-o").arg(&out_template);

    if is_playlist {
        cmd.arg("--yes-playlist");
    } else {
        cmd.arg("--no-playlist");
    }

    if video_selected {
        // Video format selection
        let format_filter = match video_quality {
            VideoQuality::Highest => "bestvideo+bestaudio/best",
            VideoQuality::FHD => "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best",
            VideoQuality::HD => "bestvideo[height<=720]+bestaudio/best[height<=720]/best",
            VideoQuality::SD => "bestvideo[height<=480]+bestaudio/best[height<=480]/best",
            VideoQuality::Lowest => "worstvideo+worstaudio/worst",
        };
        cmd.arg("-f").arg(format_filter);

        if video_container_ext == "mp4" {
            cmd.arg("--merge-output-format").arg("mp4");
            cmd.arg("--remux-video").arg("mp4");
        } else if video_container_ext == "mkv" {
            cmd.arg("--merge-output-format").arg("mkv");
            cmd.arg("--remux-video").arg("mkv");
        } else if video_container_ext == "webm" {
            cmd.arg("--merge-output-format").arg("webm");
            cmd.arg("--remux-video").arg("webm");
        }
    } else {
        // Audio extraction
        cmd.arg("-x");
        cmd.arg("--audio-format").arg(audio_ext);
        let audio_q = match audio_quality {
            AudioQuality::Best => "0",
            AudioQuality::High => "2",
            AudioQuality::Medium => "5",
            AudioQuality::Low => "7",
            AudioQuality::Worst => "9",
        };
        cmd.arg("--audio-quality").arg(audio_q);
    }

    cmd.arg(url);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => {
            tokio::spawn(async move {
                let _ = notify.summary(&fl_str!("download-failed", title = display_title)).show_async().await;
            });
            let _ = output.send(cosmic::Action::App(Message::Finished(download_id))).await;
            return;
        }
    };

    if let Some(stdout) = child.stdout.take() {
        let mut reader = tokio::io::BufReader::new(stdout).lines();
        let mut progress_out = output.clone();

        tokio::spawn(async move {
            use cosmic::iced::futures::SinkExt as _;
            while let Ok(Some(line)) = reader.next_line().await {
                if line.contains("[Merger]") || line.contains("[ExtractAudio]") || line.contains("[Fixup]") || line.contains("[ffmpeg]") {
                    let _ = progress_out.send(cosmic::Action::App(Message::DownloadProgress {
                        id: download_id,
                        percent: 100.0,
                        speed_mbps: 0.0,
                        eta_secs: None,
                        downloaded_bytes: 0,
                        total_bytes: 0,
                        is_post_processing: true,
                    })).await;
                } else if line.contains("[download] Downloading item") || line.contains("[download] Downloading video") {
                    // Playlist item progress
                    if let Some(item_idx) = line.find("item ") {
                        let after = &line[item_idx + 5..];
                        if let Some(of_idx) = after.find(" of ") {
                            let cur = after[..of_idx].trim().parse::<u32>().unwrap_or(1);
                            let tot = after[of_idx + 4..].split_whitespace().next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
                            let _ = progress_out.send(cosmic::Action::App(Message::PlaylistProgress {
                                id: download_id,
                                current: cur,
                                total: tot,
                                video_title: String::new(),
                            })).await;
                        }
                    }
                } else if let Some((percent, speed_mbps, eta_secs, dl_bytes, tot_bytes)) = parse_ytdlp_progress_line(&line) {
                    let _ = progress_out.send(cosmic::Action::App(Message::DownloadProgress {
                        id: download_id,
                        percent,
                        speed_mbps,
                        eta_secs,
                        downloaded_bytes: dl_bytes,
                        total_bytes: tot_bytes,
                        is_post_processing: false,
                    })).await;
                }
            }
        });
    }

    let status = child.wait().await;
    cleanup_zero_and_temp_files(output_dir_ref).await;

    if status.map_or(false, |s| s.success()) {
        tokio::spawn(async move {
            let _ = notify.summary(&fl_str!("finished-download", title = display_title)).show_async().await;
        });
    } else {
        tokio::spawn(async move {
            let _ = notify.summary(&fl_str!("download-failed", title = display_title)).show_async().await;
        });
    }

    let _ = output.send(cosmic::Action::App(Message::Finished(download_id))).await;
}

// ---------------------------------------------------------------------------
// Static dropdown option lists
// ---------------------------------------------------------------------------

const VIDEO_CONTAINERS: &[VideoContainer] = &[
    VideoContainer::MP4,
    VideoContainer::MKV,
    VideoContainer::WebM,
];
const VIDEO_CONTAINER_LABELS: &[&str] = &["MP4", "MKV", "WebM"];

const VIDEO_QUALITIES: &[VideoQuality] = &[
    VideoQuality::Highest,
    VideoQuality::FHD,
    VideoQuality::HD,
    VideoQuality::SD,
    VideoQuality::Lowest,
];
const VIDEO_QUALITY_LABELS: &[&str] = &["Highest", "1080p", "720p", "480p", "Lowest"];

const VIDEO_CODECS: &[VideoCodec] = &[
    VideoCodec::AV1,
    VideoCodec::AVC1,
    VideoCodec::VP9,
    VideoCodec::Any,
];
const VIDEO_CODEC_LABELS: &[&str] = &["AV1", "AVC1", "VP9", "Any"];

const AUDIO_QUALITIES: &[AudioQuality] = &[
    AudioQuality::Best,
    AudioQuality::High,
    AudioQuality::Medium,
    AudioQuality::Low,
    AudioQuality::Worst,
];
const AUDIO_QUALITY_LABELS: &[&str] = &["Highest", "192kbps", "128kbps", "96kbps", "Lowest"];

const AUDIO_CODECS: &[AudioCodec] = &[
    AudioCodec::MP3,
    AudioCodec::AAC,
    AudioCodec::Opus,
    AudioCodec::FLAC,
    AudioCodec::WAV,
    AudioCodec::Any,
];
const AUDIO_CODEC_LABELS: &[&str] = &["MP3", "AAC (M4A)", "Opus", "FLAC", "WAV", "Any"];

// ---------------------------------------------------------------------------
// Per-download progress state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ActiveDownload {
    pub id: u32,
    pub percent: f32,
    pub speed_mbps: f64,
    pub eta_secs: Option<u64>,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub is_post_processing: bool,
    pub is_audio: bool,
    // Playlist tracking (None for single-video downloads)
    pub playlist_current: Option<u32>,
    pub playlist_total: Option<u32>,
    pub playlist_title: Option<String>,
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Ytdlp {
    core: Core,

    download_type: SingleSelectModel,
    video_entity: Entity,

    video_folder: String,
    audio_folder: String,
    url: String,
    custom_name: String,

    video_container: VideoContainer,
    video_quality: VideoQuality,
    audio_quality: AudioQuality,
    video_codec: VideoCodec,
    audio_codec: AudioCodec,

    lib_dir: PathBuf,
    popup: Option<window::Id>,

    active_downloads: Vec<ActiveDownload>,
    next_download_id: u32,
    cancel_senders: HashMap<u32, tokio::sync::oneshot::Sender<()>>,
    show_platforms: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(window::Id),
    EnterURL(String),
    EnterCustomName(String),
    SelectFolder,
    ProcessSelectFolder(String),
    ChangeType(Entity),
    VideoContainerSelected(usize),
    VideoQualitySelected(usize),
    AudioQualitySelected(usize),
    VideoCodecSelected(usize),
    AudioCodecSelected(usize),
    Download,
    CancelDownload(u32),
    DownloadProgress {
        id: u32,
        percent: f32,
        speed_mbps: f64,
        eta_secs: Option<u64>,
        downloaded_bytes: u64,
        total_bytes: u64,
        is_post_processing: bool,
    },
    /// Update playlist per-item progress counter
    PlaylistProgress {
        id: u32,
        current: u32,
        total: u32,
        video_title: String,
    },
    Finished(u32),
    TogglePlatforms,
    /// Surface action forwarded from popup_dropdown
    SurfaceAction(cosmic::surface::Action),
}

impl Application for Ytdlp {
    type Executor = cosmic::executor::Default;
    type Flags = PathBuf;
    type Message = Message;

    const APP_ID: &'static str = "io.github.felix_the_cat177.CosmicAppletYtDlp";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let mut download_type = SingleSelectModel::default();
        let video_entity = download_type.insert().text(fl!("video")).id();
        download_type.insert().text(fl!("audio"));
        download_type.activate(video_entity);

        let video_folder = xdg_user::videos()
            .ok()
            .flatten()
            .map_or(String::from("~/Videos"), |path| {
                String::from(path.to_string_lossy())
            });
        let audio_folder = xdg_user::music()
            .ok()
            .flatten()
            .map_or(String::from("~/Music"), |path| {
                String::from(path.to_string_lossy())
            });

        let app = Ytdlp {
            core,
            download_type,
            video_entity,
            video_folder,
            audio_folder,
            lib_dir: flags,
            ..Default::default()
        };

        (app, Task::none())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button("multimedia-video-player-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: window::Id) -> Element<'_, Self::Message> {
        let video_selected = self.video_entity == self.download_type.active();
        let pad = self.core.applet.suggested_padding(true);
        let Spacing {
            space_xxs, space_s, ..
        } = cosmic::theme::active().cosmic().spacing;

        let mut content = column![
            // URL input row + platforms toggle button
            row![
                text_input(fl!("url"), &self.url)
                    .on_input(Message::EnterURL)
                    .width(Length::Fill),
                cosmic::widget::tooltip(
                    cosmic::widget::button::icon(
                        cosmic::widget::icon::from_name("help-about-symbolic")
                    )
                    .on_press(Message::TogglePlatforms),
                    cosmic::widget::text::body(fl!("platforms-tooltip")),
                    cosmic::widget::tooltip::Position::Bottom,
                )
            ]
            .align_y(Alignment::Center)
            .spacing(space_xxs)
            .apply(padded_control)
            .width(Length::Fill),
            // Custom file name input (optional)
            text_input(fl!("filename"), &self.custom_name)
                .on_input(Message::EnterCustomName)
                .apply(padded_control)
                .width(Length::Fill),
            segmented_control::horizontal(&self.download_type)
                .on_activate(Message::ChangeType)
                .apply(padded_control)
                .width(Length::Fill),
            if video_selected {
                self.view_video(self.popup)
            } else {
                self.view_audio(self.popup)
            },
            padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
            row![
                body(fl!("folder")).width(Length::Fill),
                cosmic::widget::button::standard(fl!("browse"))
                    .on_press(Message::SelectFolder)
            ]
            .align_y(Alignment::Center)
            .spacing(pad.0)
            .apply(padded_control),
            text_input(
                "",
                if video_selected {
                    self.video_folder.clone()
                } else {
                    self.audio_folder.clone()
                }
            )
            .on_focus(Message::SelectFolder)
            .on_input(Message::ProcessSelectFolder)
            .apply(padded_control),
            padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
            {
                let active_count = self.active_downloads.len() as u32;
                row![
                    body(fl!("downloading", total = active_count))
                        .width(Length::Fill),
                    cosmic::widget::button::suggested(fl!("download"))
                        .on_press(Message::Download),
                ]
                .align_y(Alignment::Center)
                .spacing(pad.0)
                .apply(padded_control)
            },
        ]
        .padding([pad.0, pad.1]);

        // Show platforms panel if toggled
        if self.show_platforms {
            content = content.push(self.view_platforms());
        }

        // Append a progress row for each active download
        for dl in &self.active_downloads {
            content = content.push(self.view_progress(dl));
        }

        let scrollable_content = cosmic::widget::scrollable(content)
            .height(Length::Shrink)
            .width(Length::Fill);

        self.core.applet.popup_container(scrollable_content).into()
    }

    #[allow(clippy::too_many_lines)]
    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    cosmic::surface::surface_task(cosmic::surface::action::app_popup(
                        |_| Default::default(),
                        move |app: &mut Self| {
                            let new_id = window::Id::unique();
                            app.popup.replace(new_id);
                            let mut popup_settings = app.core.applet.get_popup_settings(
                                app.core.main_window_id().unwrap(),
                                new_id,
                                None,
                                None,
                                None,
                            );
                            popup_settings.positioner.size_limits = Limits::NONE
                                .max_width(800.0)
                                .min_width(320.0)
                                .min_height(200.0)
                                .max_height(850.0);
                            popup_settings
                        },
                        None,
                    ))
                };
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::EnterURL(url) => self.url = url,
            Message::EnterCustomName(name) => self.custom_name = name,
            Message::ChangeType(id) => self.download_type.activate(id),
            Message::VideoContainerSelected(idx) => {
                if let Some(&c) = VIDEO_CONTAINERS.get(idx) {
                    self.video_container = c;
                }
            }
            Message::VideoQualitySelected(idx) => {
                if let Some(&q) = VIDEO_QUALITIES.get(idx) {
                    self.video_quality = q;
                }
            }
            Message::AudioQualitySelected(idx) => {
                if let Some(&q) = AUDIO_QUALITIES.get(idx) {
                    self.audio_quality = q;
                }
            }
            Message::VideoCodecSelected(idx) => {
                if let Some(&c) = VIDEO_CODECS.get(idx) {
                    self.video_codec = c;
                }
            }
            Message::AudioCodecSelected(idx) => {
                if let Some(&c) = AUDIO_CODECS.get(idx) {
                    self.audio_codec = c;
                }
            }
            Message::SelectFolder => {
                let future = async {
                    let request = SelectedFiles::open_file()
                        .title("Download Folder")
                        .accept_label("Select")
                        .directory(true)
                        .multiple(false)
                        .modal(true)
                        .send()
                        .await
                        .ok()?;
                    let folder = request.response().ok()?;
                    let uri = folder.uris().first()?;
                    uri.to_file_path().ok().map(|p| p.to_string_lossy().into_owned())
                };
                return Task::perform(future, |folder| {
                    if let Some(folder) = folder {
                        return Action::App(Message::ProcessSelectFolder(folder));
                    }
                    Action::App(Message::TogglePopup)
                });
            }
            Message::ProcessSelectFolder(folder) => {
                let video_selected = self.video_entity == self.download_type.active();
                if video_selected {
                    self.video_folder = folder;
                } else {
                    self.audio_folder = folder;
                }
                return Task::done(Action::App(Message::TogglePopup));
            }
            Message::CancelDownload(id) => {
                if let Some(cancel_tx) = self.cancel_senders.remove(&id) {
                    let _ = cancel_tx.send(());
                }
                self.active_downloads.retain(|d| d.id != id);
                let mut notify = Notification::new()
                    .appname("yt-dlp applet")
                    .icon("multimedia-video-player-symbolic")
                    .finalize();
                tokio::spawn(async move {
                    let _ = notify.summary(&fl_str!("download-cancelled", title = "")).show_async().await;
                });
            }
            Message::Download => {
                let video_selected = self.video_entity == self.download_type.active();

                let download_id = self.next_download_id;
                self.next_download_id += 1;

                self.active_downloads.push(ActiveDownload {
                    id: download_id,
                    percent: 0.0,
                    speed_mbps: 0.0,
                    eta_secs: None,
                    downloaded_bytes: 0,
                    total_bytes: 0,
                    is_post_processing: false,
                    is_audio: !video_selected,
                    playlist_current: None,
                    playlist_total: None,
                    playlist_title: None,
                });

                let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
                self.cancel_senders.insert(download_id, cancel_tx);

                let url = self.url.clone();
                let custom_name = if self.custom_name.trim().is_empty() {
                    None
                } else {
                    Some(self.custom_name.clone())
                };
                let lib_dir = self.lib_dir.clone();
                self.url.clear();
                self.custom_name.clear();
                let output_dir = PathBuf::from(if video_selected {
                    &self.video_folder
                } else {
                    &self.audio_folder
                });
                let output_dir_ref = output_dir.clone();
                let video_container_ext = self.video_container.extension();
                let audio_ext = self.audio_codec.extension();
                let video_quality = self.video_quality;
                let audio_quality = self.audio_quality;

                return Task::stream(cosmic::iced::stream::channel(
                    64,
                    move |mut output: cosmic::iced::futures::channel::mpsc::Sender<Action<Message>>| async move {
                        let mut notify = Notification::new()
                            .appname("yt-dlp applet")
                            .icon("multimedia-video-player-symbolic")
                            .finalize();

                        let downloader =
                            fetcher::with_output_dir(&lib_dir, output_dir).await;

                        tokio::select! {
                            _ = &mut cancel_rx => {
                                cleanup_zero_and_temp_files(&output_dir_ref).await;
                                let _ = output.send(Action::App(Message::Finished(download_id))).await;
                            }
                            _ = async {
                                run_download_job(
                                    download_id,
                                    video_selected,
                                    &url,
                                    custom_name,
                                    video_container_ext,
                                    &downloader,
                                    &output_dir_ref,
                                    audio_ext,
                                    video_quality,
                                    audio_quality,
                                    &mut output,
                                    notify,
                                ).await;
                            } => {}
                        }
                    },
                ));
            }
            Message::DownloadProgress {
                id,
                percent,
                speed_mbps,
                eta_secs,
                downloaded_bytes,
                total_bytes,
                is_post_processing,
            } => {
                if let Some(dl) = self.active_downloads.iter_mut().find(|d| d.id == id) {
                    dl.percent = percent;
                    dl.speed_mbps = speed_mbps;
                    dl.eta_secs = eta_secs;
                    dl.downloaded_bytes = downloaded_bytes;
                    dl.total_bytes = total_bytes;
                    dl.is_post_processing = is_post_processing;
                }
            }
            Message::PlaylistProgress { id, current, total, video_title } => {
                if let Some(dl) = self.active_downloads.iter_mut().find(|d| d.id == id) {
                    dl.playlist_current = Some(current);
                    dl.playlist_total = Some(total);
                    if !video_title.is_empty() {
                        dl.playlist_title = Some(video_title);
                    }
                    dl.percent = 0.0;
                    dl.downloaded_bytes = 0;
                    dl.total_bytes = 0;
                    dl.eta_secs = None;
                    dl.is_post_processing = false;
                }
            }
            Message::Finished(id) => {
                self.cancel_senders.remove(&id);
                self.active_downloads.retain(|d| d.id != id);
            }
            Message::SurfaceAction(action) => {
                return cosmic::surface::surface_task(action);
            }
            Message::TogglePlatforms => {
                self.show_platforms = !self.show_platforms;
            }
        }
        Task::none()
    }
}

// ---------------------------------------------------------------------------
// View helpers
// ---------------------------------------------------------------------------

impl Ytdlp {
    /// Renders the expandable list of supported video & audio platforms.
    fn view_platforms(&self) -> Element<'_, Message> {
        let Spacing {
            space_xxs, space_xs, space_s, ..
        } = cosmic::theme::active().cosmic().spacing;

        column![
            padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
            row![
                cosmic::widget::text::body("▶ YouTube"),
                cosmic::widget::text::body("🎵 TikTok"),
                cosmic::widget::text::body("📸 Instagram"),
            ]
            .spacing(space_s)
            .apply(padded_control),
            row![
                cosmic::widget::text::body("𝕏 Twitter/X"),
                cosmic::widget::text::body("🟣 Twitch"),
                cosmic::widget::text::body("🟠 SoundCloud"),
            ]
            .spacing(space_s)
            .apply(padded_control),
            row![
                cosmic::widget::text::body("🔵 Facebook"),
                cosmic::widget::text::body("🔴 Reddit"),
                cosmic::widget::text::body("🟢 Spotify"),
            ]
            .spacing(space_s)
            .apply(padded_control),
            cosmic::widget::text::caption(fl!("platforms-footer"))
                .apply(padded_control),
            padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
        ]
        .spacing(space_xs)
        .into()
    }

    fn view_video(&self, popup_id: Option<window::Id>) -> Element<'_, Message> {
        let container_idx = VIDEO_CONTAINERS
            .iter()
            .position(|&c| c == self.video_container);
        let video_quality_idx = VIDEO_QUALITIES
            .iter()
            .position(|&q| q == self.video_quality);
        let video_codec_idx = VIDEO_CODECS.iter().position(|&c| c == self.video_codec);

        let Spacing { space_xxs, .. } = cosmic::theme::active().cosmic().spacing;

        let container_dropdown: Element<'_, Message> = if let Some(pid) = popup_id {
            Element::from(
                cosmic::widget::dropdown::popup_dropdown(
                    VIDEO_CONTAINER_LABELS,
                    container_idx,
                    Message::VideoContainerSelected,
                    pid,
                    Message::SurfaceAction,
                    |m| m,
                )
                .width(Length::FillPortion(1)),
            )
        } else {
            Element::from(
                cosmic::widget::dropdown(
                    VIDEO_CONTAINER_LABELS,
                    container_idx,
                    Message::VideoContainerSelected,
                )
                .width(Length::FillPortion(1)),
            )
        };

        let quality_dropdown: Element<'_, Message> = if let Some(pid) = popup_id {
            Element::from(
                cosmic::widget::dropdown::popup_dropdown(
                    VIDEO_QUALITY_LABELS,
                    video_quality_idx,
                    Message::VideoQualitySelected,
                    pid,
                    Message::SurfaceAction,
                    |m| m,
                )
                .width(Length::FillPortion(1)),
            )
        } else {
            Element::from(
                cosmic::widget::dropdown(
                    VIDEO_QUALITY_LABELS,
                    video_quality_idx,
                    Message::VideoQualitySelected,
                )
                .width(Length::FillPortion(1)),
            )
        };

        let codec_dropdown: Element<'_, Message> = if let Some(pid) = popup_id {
            Element::from(
                cosmic::widget::dropdown::popup_dropdown(
                    VIDEO_CODEC_LABELS,
                    video_codec_idx,
                    Message::VideoCodecSelected,
                    pid,
                    Message::SurfaceAction,
                    |m| m,
                )
                .width(Length::FillPortion(1)),
            )
        } else {
            Element::from(
                cosmic::widget::dropdown(
                    VIDEO_CODEC_LABELS,
                    video_codec_idx,
                    Message::VideoCodecSelected,
                )
                .width(Length::FillPortion(1)),
            )
        };

        column![
            row![
                body(fl!("video-format")).width(Length::FillPortion(1)),
                container_dropdown,
            ]
            .align_y(Alignment::Center)
            .spacing(space_xxs)
            .apply(padded_control),
            row![
                body(fl!("video-quality")).width(Length::FillPortion(1)),
                quality_dropdown,
            ]
            .align_y(Alignment::Center)
            .spacing(space_xxs)
            .apply(padded_control),
            row![
                body(fl!("video-codec")).width(Length::FillPortion(1)),
                codec_dropdown,
            ]
            .align_y(Alignment::Center)
            .spacing(space_xxs)
            .apply(padded_control),
        ]
        .into()
    }

    fn view_audio(&self, popup_id: Option<window::Id>) -> Element<'_, Message> {
        let audio_quality_idx = AUDIO_QUALITIES
            .iter()
            .position(|&q| q == self.audio_quality);
        let audio_codec_idx = AUDIO_CODECS.iter().position(|&c| c == self.audio_codec);

        let Spacing { space_xxs, .. } = cosmic::theme::active().cosmic().spacing;

        let quality_dropdown: Element<'_, Message> = if let Some(pid) = popup_id {
            Element::from(
                cosmic::widget::dropdown::popup_dropdown(
                    AUDIO_QUALITY_LABELS,
                    audio_quality_idx,
                    Message::AudioQualitySelected,
                    pid,
                    Message::SurfaceAction,
                    |m| m,
                )
                .width(Length::FillPortion(1)),
            )
        } else {
            Element::from(
                cosmic::widget::dropdown(
                    AUDIO_QUALITY_LABELS,
                    audio_quality_idx,
                    Message::AudioQualitySelected,
                )
                .width(Length::FillPortion(1)),
            )
        };

        let codec_dropdown: Element<'_, Message> = if let Some(pid) = popup_id {
            Element::from(
                cosmic::widget::dropdown::popup_dropdown(
                    AUDIO_CODEC_LABELS,
                    audio_codec_idx,
                    Message::AudioCodecSelected,
                    pid,
                    Message::SurfaceAction,
                    |m| m,
                )
                .width(Length::FillPortion(1)),
            )
        } else {
            Element::from(
                cosmic::widget::dropdown(
                    AUDIO_CODEC_LABELS,
                    audio_codec_idx,
                    Message::AudioCodecSelected,
                )
                .width(Length::FillPortion(1)),
            )
        };

        column![
            row![
                body(fl!("audio-codec")).width(Length::FillPortion(1)),
                codec_dropdown,
            ]
            .align_y(Alignment::Center)
            .spacing(space_xxs)
            .apply(padded_control),
            row![
                body(fl!("audio-quality")).width(Length::FillPortion(1)),
                quality_dropdown,
            ]
            .align_y(Alignment::Center)
            .spacing(space_xxs)
            .apply(padded_control),
        ]
        .into()
    }

    fn view_progress<'a>(&self, dl: &'a ActiveDownload) -> Element<'a, Message> {
        let Spacing {
            space_xxs, space_xs, space_s, ..
        } = cosmic::theme::active().cosmic().spacing;

        // Playlist header line: "Playlist 3/15 – Some Video Title"
        let playlist_line = if let (Some(cur), Some(tot)) =
            (dl.playlist_current, dl.playlist_total)
        {
            let title = dl.playlist_title.clone().unwrap_or_default();
            Some(fl!("playlist-downloading",
                current = cur,
                total = tot,
                title = title
            ))
        } else {
            None
        };

        let status_line = if dl.is_post_processing {
            if dl.is_audio {
                fl!("post-processing-audio")
            } else {
                fl!("post-processing")
            }
        } else {
            let eta_text = match dl.eta_secs {
                Some(secs) => {
                    let mins = secs / 60;
                    let s = secs % 60;
                    if mins > 0 {
                        fl!("eta-mins-secs", mins = format!("{:02}", mins), secs = format!("{:02}", s))
                    } else {
                        fl!("eta-secs", secs = s)
                    }
                }
                None => fl!("calculating"),
            };
            format!(
                "{:.1} MB/s  ──  {:.0}%  ──  {}",
                dl.speed_mbps, dl.percent, eta_text
            )
        };

        // Format file size: "1.2 MB / 45.6 MB" or "1.2 MB"
        let size_text = if dl.total_bytes > 0 {
            let dl_mb = dl.downloaded_bytes as f64 / 1_048_576.0;
            let total_mb = dl.total_bytes as f64 / 1_048_576.0;
            format!("{:.1} MB / {:.1} MB", dl_mb, total_mb)
        } else if dl.downloaded_bytes > 0 {
            let dl_mb = dl.downloaded_bytes as f64 / 1_048_576.0;
            format!("{:.1} MB", dl_mb)
        } else {
            String::new()
        };

        let cancel_btn = cosmic::widget::tooltip(
            cosmic::widget::button::icon(
                cosmic::widget::icon::from_name("process-stop-symbolic")
            )
            .on_press(Message::CancelDownload(dl.id)),
            cosmic::widget::text::body(fl!("cancel")),
            cosmic::widget::tooltip::Position::Left,
        );

        let mut col = column![].spacing(space_xxs);

        if let Some(pl_line) = playlist_line {
            col = col.push(cosmic::widget::text::caption(pl_line));
        }

        col = col
            .push(
                row![
                    column![
                        cosmic::widget::text::caption(status_line),
                        cosmic::widget::determinate_linear(dl.percent / 100.0)
                            .width(Length::Fill)
                            .girth(6),
                        if !size_text.is_empty() {
                            cosmic::widget::text::caption(size_text)
                        } else {
                            cosmic::widget::text::caption("")
                        }
                    ]
                    .spacing(space_xxs)
                    .width(Length::Fill),
                    cancel_btn,
                ]
                .align_y(Alignment::Center)
                .spacing(space_xs),
            );

        col.padding([0, space_s]).into()
    }
}
