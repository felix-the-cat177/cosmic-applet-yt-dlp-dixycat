# COSMIC Applet yt-dlp

<p align="center">
  <a href="./README.md"><b>English</b></a> |
  <a href="./README.pt-BR.md"><b>Português (Brasil)</b></a> |
  <a href="./README.es.md"><b>Español</b></a>
</p>

<p align="center">
  Un applet moderno y completo con interfaz para <b>yt-dlp</b> desarrollado en Rust para el entorno de escritorio <b>COSMIC Desktop Environment</b>.
</p>

<p align="center">
  <img src="./res/applet-photo2.png" alt="Captura de pantalla de COSMIC Applet yt-dlp" width="600" />
</p>

---

## ✨ Características

- 🎨 **Diseño Nativo de COSMIC y Efecto Cristal Esmerilado (Blur)**: Se integra a la perfección con el tema del sistema, respetando las preferencias de transparencia y desenfoque de COSMIC.
- 🎬 **Descargas de Video y Audio**: Descarga videos (Highest, 1080p, 720p, 480p) o extrae pistas de audio con selección de calidad y códec (Opus, AAC, MP3, Any).
- 📑 **Soporte Completo para Listas de Reproducción (Playlists)**: Descarga listas completas con indicador individual de pista y progreso en tiempo real (`Playlist 3/4 - Título`).
- ⚡ **Estadísticas de Descarga en Tiempo Real**:
  - Velocidad actual de descarga (ej: `2.23 Mb/s`)
  - Barra de progreso lineal con porcentaje (`31%`)
  - Tiempo estimado restante / ETA (`estimado: 1s`)
  - Tamaño descargado vs tamaño total (`1.2 MB / 3.8 MB`)
- 📁 **Carpeta de Descarga Personalizada**: Elige carpetas de destino para videos y música con el selector de archivos nativo del sistema.
- 🔔 **Notificaciones del Sistema**: Alertas en el escritorio al iniciar, finalizar o si ocurre algún error.

---

## 📦 Instalación

### Paquetes Precompilados (.deb / .rpm)

Descarga la versión más reciente para tu arquitectura (`x86_64` o `arm64`) desde la página de [Releases](https://github.com/felix-the-cat177/cosmic-applet-yt-dlp-dixycat/releases/latest).

#### Debian / Ubuntu / Pop!_OS:
```bash
sudo apt install ./cosmic-applet-yt-dlp-dixycat_*.deb
```

#### Fedora / openSUSE / distribuciones basadas en RPM:
```bash
sudo rpm -i ./cosmic-applet-yt-dlp-dixycat-*.rpm
```

---

## 🛠️ Compilación desde el Código Fuente

### Dependencias
Asegúrate de tener instalados Rust, `just` y `libxkbcommon-dev`:
```bash
sudo apt install -y rustc cargo just libxkbcommon-dev
```

### Compilar e Instalar
```bash
git clone https://github.com/felix-the-cat177/cosmic-applet-yt-dlp-dixycat.git
cd cosmic-applet-yt-dlp-dixycat

# Compilar en modo release
just build-release

# Instalar en el sistema
sudo just install
# o solo para el usuario actual:
just install-local
```

### Recetas de Empaquetado
- `just build-deb` — Genera el paquete Debian `.deb`
- `just build-rpm` — Genera el paquete RedHat `.rpm`

---

## 📄 Licencia

Distribuido bajo la licencia [GPL-3.0](./LICENSE).
