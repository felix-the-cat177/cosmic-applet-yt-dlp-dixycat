// SPDX-License-Identifier: GPL-3.0-only

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

use yt_dlp::extractor::VideoExtractor;
use yt_dlp::VideoSelection;

use crate::formats::{AudioCodec, AudioQuality, VideoCodec, VideoQuality};
use crate::{fetcher, fl, fl_str};

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Returns a unique filename in `dir` by appending `-2`, `-3`, … if the file
/// already exists.
async fn unique_path(dir: &std::path::Path, filename: &str) -> std::path::PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let stem = std::path::Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let mut n = 2u32;
    loop {
        let name = if ext.is_empty() {
            format!("{stem}-{n}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        let candidate = dir.join(&name);
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Removes leftover temporary files (`temp_video_*` and `temp_audio_*`) from
/// `dir`. Errors are silently ignored — these are best-effort cleanups.
async fn cleanup_temp_files(dir: &std::path::Path) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    let mut tasks = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with("temp_video_") || name.starts_with("temp_audio_") {
            tasks.push(tokio::fs::remove_file(entry.path()));
        }
    }
    for task in tasks {
        let _ = task.await;
    }
}

// ---------------------------------------------------------------------------
// Static dropdown option lists
// ---------------------------------------------------------------------------

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
    AudioCodec::Opus,
    AudioCodec::AAC,
    AudioCodec::MP3,
    AudioCodec::Any,
];
const AUDIO_CODEC_LABELS: &[&str] = &["Opus", "AAC", "MP3", "Any"];

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

    video_quality: VideoQuality,
    audio_quality: AudioQuality,
    video_codec: VideoCodec,
    audio_codec: AudioCodec,

    lib_dir: PathBuf,
    popup: Option<window::Id>,

    active_downloads: Vec<ActiveDownload>,
    next_download_id: u32,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(window::Id),
    EnterURL(String),
    SelectFolder,
    ProcessSelectFolder(String),
    ChangeType(Entity),
    VideoQualitySelected(usize),
    AudioQualitySelected(usize),
    VideoCodecSelected(usize),
    AudioCodecSelected(usize),
    Download,
    DownloadProgress {
        id: u32,
        percent: f32,
        speed_mbps: f64,
        eta_secs: Option<u64>,
        downloaded_bytes: u64,
        total_bytes: u64,
        is_post_processing: bool,
    },
    Finished(u32),
    /// Surface action forwarded from popup_dropdown
    SurfaceAction(cosmic::surface::Action),
}

impl Application for Ytdlp {
    type Executor = cosmic::executor::Default;
    type Flags = PathBuf;
    type Message = Message;

    const APP_ID: &'static str = "dev.DBrox.CosmicYtdlp";

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
            text_input(fl!("url"), &self.url)
                .on_input(Message::EnterURL)
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

        // Append a progress row for each active download
        for dl in &self.active_downloads {
            content = content.push(self.view_progress(dl));
        }

        self.core.applet.popup_container(content).into()
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
                                .min_width(300.0)
                                .min_height(200.0)
                                .max_height(1000.0);
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
            Message::ChangeType(id) => self.download_type.activate(id),
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
                    Some(uri.path().to_string())
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
                });

                let url = self.url.clone();
                let lib_dir = self.lib_dir.clone();
                self.url.clear();
                let output_dir = PathBuf::from(if video_selected {
                    &self.video_folder
                } else {
                    &self.audio_folder
                });
                let output_dir_ref = output_dir.clone();
                let audio_ext = self.audio_codec.extension();
                let video_quality: yt_dlp::model::selector::VideoQuality =
                    self.video_quality.into();
                let video_codec: yt_dlp::model::selector::VideoCodecPreference =
                    self.video_codec.into();
                let audio_quality: yt_dlp::model::selector::AudioQuality =
                    self.audio_quality.into();
                let audio_codec: yt_dlp::model::selector::AudioCodecPreference =
                    self.audio_codec.into();

                return Task::stream(cosmic::iced::stream::channel(
                    32,
                    move |mut output: cosmic::iced::futures::channel::mpsc::Sender<Action<Message>>| async move {
                        use cosmic::iced::futures::SinkExt;

                        let mut notify = Notification::new()
                            .appname("yt-dlp applet")
                            .icon("multimedia-video-player-symbolic")
                            .finalize();

                        let downloader =
                            fetcher::with_output_dir(&lib_dir, output_dir).await;

                        // Subscribe before starting download so we catch all events
                        let mut event_rx = downloader.subscribe_events();

                        let video =
                            match downloader.generic_extractor().fetch_video(&url).await {
                                Ok(v) => v,
                                Err(_) => {
                                    let _ = notify
                                        .summary(fl_str!("metadata-failed"))
                                        .show_async()
                                        .await;
                                    let _ = output
                                        .send(Action::App(Message::Finished(download_id)))
                                        .await;
                                    return;
                                }
                            };

                        let title = video.title.clone();

                        // Resolve the output filename (deduplicate if it already exists)
                        let output_filename = if video_selected {
                            unique_path(&output_dir_ref, &format!("{title}.mp4")).await
                        } else {
                            unique_path(&output_dir_ref, &format!("{title}.{audio_ext}")).await
                        };
                        let output_filename_str = output_filename
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&title)
                            .to_string();

                        let has_format = if video_selected {
                            video
                                .select_video_format(video_quality, video_codec.clone())
                                .is_some()
                        } else {
                            video
                                .select_audio_format(audio_quality, audio_codec.clone())
                                .is_some()
                        };

                        if !has_format {
                            let _ = notify
                                .summary(fl_str!("missing-format"))
                                .show_async()
                                .await;
                            let _ = output
                                .send(Action::App(Message::Finished(download_id)))
                                .await;
                            return;
                        }

                        // Spawn a task to forward progress events to the UI
                        let mut progress_output = output.clone();
                        tokio::spawn(async move {
                            use std::collections::HashMap;
                            use cosmic::iced::futures::SinkExt as _;
                            use yt_dlp::events::DownloadEvent;

                            let mut streams: HashMap<u64, (u64, u64)> = HashMap::new();

                            while let Ok(event) = event_rx.recv().await {
                                match &*event {
                                    DownloadEvent::DownloadProgress {
                                        download_id: stream_id,
                                        downloaded_bytes,
                                        total_bytes,
                                        speed_bytes_per_sec,
                                        eta_seconds,
                                    } => {
                                        streams.insert(*stream_id, (*downloaded_bytes, *total_bytes));

                                        let sum_downloaded: u64 = streams.values().map(|(d, _)| d).sum();
                                        let sum_total: u64 = streams.values().map(|(_, t)| t).sum();

                                        let percent = if sum_total > 0 {
                                            (sum_downloaded as f32 / sum_total as f32) * 100.0
                                        } else {
                                            0.0
                                        };

                                        let speed_mbps = speed_bytes_per_sec / 1_000_000.0;

                                        let eta_secs = if let Some(eta) = eta_seconds {
                                            if *eta > 0 { Some(*eta) } else { None }
                                        } else { None };

                                        let final_eta = eta_secs.or_else(|| {
                                            if *speed_bytes_per_sec > 10.0 && sum_total > sum_downloaded {
                                                let rem_bytes = sum_total - sum_downloaded;
                                                Some((rem_bytes as f64 / speed_bytes_per_sec) as u64)
                                            } else {
                                                None
                                            }
                                        });

                                        let _ = progress_output.send(Action::App(
                                            Message::DownloadProgress {
                                                id: download_id,
                                                percent,
                                                speed_mbps,
                                                eta_secs: final_eta,
                                                downloaded_bytes: sum_downloaded,
                                                total_bytes: sum_total,
                                                is_post_processing: false,
                                            },
                                        )).await;
                                    }
                                    DownloadEvent::PostProcessStarted { .. } => {
                                        let sum_total: u64 = streams.values().map(|(_, t)| t).sum();
                                        let _ = progress_output.send(Action::App(
                                            Message::DownloadProgress {
                                                id: download_id,
                                                percent: 100.0,
                                                speed_mbps: 0.0,
                                                eta_secs: None,
                                                downloaded_bytes: sum_total,
                                                total_bytes: sum_total,
                                                is_post_processing: true,
                                            },
                                        )).await;
                                    }
                                    _ => {}
                                }
                            }
                        });

                        // Run the actual download
                        let result: Result<String, ()> = if video_selected {
                            match downloader
                                .download(&video, output_filename_str.clone())
                                .video_quality(video_quality)
                                .video_codec(video_codec)
                                .audio_quality(audio_quality)
                                .audio_codec(audio_codec)
                                .execute()
                                .await
                            {
                                Ok(_) => Ok(output_filename_str.clone()),
                                Err(_) => Err(()),
                            }
                        } else {
                            // Enqueue the audio stream through the download manager so that
                            // progress events are emitted on the event bus. The standalone
                            // `download_audio_stream_with_quality` path bypasses the manager
                            // entirely and therefore never reports progress (stuck at 0%).
                            let audio_format = match video.select_audio_format(audio_quality, audio_codec) {
                                Some(f) => f,
                                None => {
                                    let _ = output
                                        .send(Action::App(Message::Finished(download_id)))
                                        .await;
                                    return;
                                }
                            };
                            let Some(audio_url) = audio_format.download_info.url.as_deref() else {
                                let _ = output
                                    .send(Action::App(Message::Finished(download_id)))
                                    .await;
                                return;
                            };
                            let headers = audio_format.download_info.http_headers.clone();
                            let dl_id = downloader
                                .download_manager()
                                .enqueue_with_headers(
                                    audio_url,
                                    output_filename,
                                    None,
                                    Some(headers),
                                )
                                .await;
                            match downloader
                                .download_manager()
                                .wait_for_completion(dl_id)
                                .await
                            {
                                Some(yt_dlp::DownloadStatus::Completed) => Ok(output_filename_str.clone()),
                                _ => Err(()),
                            }
                        };

                        // Clean up leftover temp files from the yt-dlp crate
                        cleanup_temp_files(&downloader.output_dir()).await;

                        if result.is_err() {
                            tokio::spawn(async move {
                                let _ = notify
                                    .summary(fl_str!("download-failed", title = title))
                                    .show_async()
                                    .await;
                            });
                        } else {
                            tokio::spawn(async move {
                                let _ = notify
                                    .summary(fl_str!("finished-download", title = title))
                                    .show_async()
                                    .await;
                            });
                        }
                        let _ = output
                            .send(Action::App(Message::Finished(download_id)))
                            .await;
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
            Message::Finished(id) => {
                self.active_downloads.retain(|d| d.id != id);
            }
            Message::SurfaceAction(action) => {
                return cosmic::surface::surface_task(action);
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl Ytdlp {
    fn view_video(&self, popup_id: Option<window::Id>) -> Element<'_, Message> {
        let video_quality_idx = VIDEO_QUALITIES
            .iter()
            .position(|&q| q == self.video_quality);
        let video_codec_idx = VIDEO_CODECS.iter().position(|&c| c == self.video_codec);

        let Spacing { space_xxs, .. } = cosmic::theme::active().cosmic().spacing;

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
                body(fl!("audio-quality")).width(Length::FillPortion(1)),
                quality_dropdown,
            ]
            .align_y(Alignment::Center)
            .spacing(space_xxs)
            .apply(padded_control),
            row![
                body(fl!("audio-codec")).width(Length::FillPortion(1)),
                codec_dropdown,
            ]
            .align_y(Alignment::Center)
            .spacing(space_xxs)
            .apply(padded_control),
        ]
        .into()
    }

    fn view_progress<'a>(&self, dl: &'a ActiveDownload) -> Element<'a, Message> {
        let Spacing {
            space_xxs, space_s, ..
        } = cosmic::theme::active().cosmic().spacing;

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
                "{:.2} Mb/s  ──  {:.0}%  ──  {}",
                dl.speed_mbps, dl.percent, eta_text
            )
        };

        // Format file size: "1.23 MB / 45.6 MB"
        let size_text = if dl.total_bytes > 0 {
            let dl_mb = dl.downloaded_bytes as f64 / 1_048_576.0;
            let total_mb = dl.total_bytes as f64 / 1_048_576.0;
            format!("{:.1} MB / {:.1} MB", dl_mb, total_mb)
        } else {
            String::new()
        };

        let mut col = column![
            cosmic::widget::text::caption(status_line),
            cosmic::widget::determinate_linear(dl.percent / 100.0)
                .width(Length::Fill)
                .girth(6),
        ]
        .spacing(space_xxs)
        .padding([0, space_s]);

        if !size_text.is_empty() {
            col = col.push(cosmic::widget::text::caption(size_text));
        }

        col.into()
    }
}
