#!/usr/bin/env bash

mkdir -p ~/.local/bin/ ${XDG_DATA_HOME:-~/.local/share}/{applications,metainfo,icons/hicolor/scalable/apps}/
cp ./cosmic-applet-yt-dlp-dixycat ~/.local/bin/
cp ./*.desktop ${XDG_DATA_HOME:-~/.local/share}/applications/
cp ./*.metainfo.xml ${XDG_DATA_HOME:-~/.local/share}/metainfo/
cp -r ./icons/hicolor/scalable/apps/*.svg ${XDG_DATA_HOME:-~/.local/share}/icons/hicolor/scalable/apps/ 2>/dev/null || true