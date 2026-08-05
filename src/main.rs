// SPDX-License-Identifier: GPL-3.0-only

mod applet;
mod fetcher;
mod formats;
mod i18n;

fn main() -> cosmic::iced::Result {
    // Get the system's preferred languages.
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    // Enable localizations to be applied.
    i18n::init(&requested_languages);

    let flags = tokio::runtime::Runtime::new()
        .expect("Failed to create Tokio runtime")
        .block_on(fetcher::binaries());

    cosmic::applet::run::<applet::Ytdlp>(flags)
}
