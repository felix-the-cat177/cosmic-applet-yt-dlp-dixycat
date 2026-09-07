## 🚀 O que há de novo na v0.2.7 / What's New in v0.2.7

### 🇧🇷 Português
- 🔄 **Auto-Atualização Automática do yt-dlp:** O applet agora verifica e baixa automaticamente a versão mais recente do `yt-dlp` em segundo plano ao iniciar, garantindo compatibilidade contínua com mudanças do YouTube sem necessidade de atualizar o applet manualmente.
- ⚠️ **Notificação de Atualização em Caso de Falha:** Se um download falhar devido a mudanças no YouTube ou erros do extrator, o applet tenta atualizar o `yt-dlp` automaticamente e notifica o usuário para tentar novamente.
- 🔍 **Detecção Proativa de Versões:** Verificação da versão instalada do `yt-dlp` e comparação com a versão estável mais recente nos releases oficiais do GitHub.

---

### 🇺🇸 English
- 🔄 **Automatic yt-dlp Self-Update:** The applet now automatically checks and downloads the latest `yt-dlp` version in the background on startup, ensuring continuous compatibility with YouTube changes without requiring manual applet updates.
- ⚠️ **Update Notification on Failure:** If a download fails due to YouTube changes or extractor errors, the applet attempts to auto-update `yt-dlp` and notifies the user to retry.
- 🔍 **Proactive Version Detection:** Checks installed `yt-dlp` version against the latest stable release from official GitHub releases.

---

### 🇪🇸 Español
- 🔄 **Auto-Actualización Automática de yt-dlp:** La aplicación ahora verifica y descarga automáticamente la última versión de `yt-dlp` en segundo plano al iniciar.
- ⚠️ **Notificación de Actualización en Caso de Fallo:** Si una descarga falla, la aplicación intenta actualizar `yt-dlp` automáticamente.
- 🔍 **Detección Proactiva de Versiones:** Verificación de la versión instalada de `yt-dlp`.

---

## 🚀 O que há de novo na v0.2.6 / What's New in v0.2.6

### 🇧🇷 Português
- 🎨 **Ícone no Painel e Dock Corrigido:** Os ícones SVG (normal e simbólico) agora são empacotados corretamente dentro dos pacotes `.deb`, `.rpm` e `.tar.gz`. O ícone agora aparece nas Configurações do COSMIC, na Dock e no Painel superior.
- 🛡️ **Correção da Falha de Inicialização (`ChecksumMismatch`):** O applet não trava mais com erro de checksum ao tentar baixar o FFmpeg. Agora ele detecta automaticamente o FFmpeg do sistema e trata downloads com tolerância a falhas.
- 🎬 **Downloads do YouTube (Vídeo e Música) Restaurados:**
  - Contorno do bloqueio SABR do YouTube com fallback para clientes `default,web_embedded,ios`.
  - Correção na integração do FFmpeg e suporte a `ffprobe` para incorporação de miniaturas/capa sem travamentos de pós-processamento.
  - Download e validação automática do binário oficial mais recente do `yt-dlp`.
- 📦 **Dependência Oficial do FFmpeg no Pacote DEB:** O pacote `.deb` agora declara `ffmpeg` como dependência, instalando-o nativamente pelo gerenciador de pacotes do sistema (`apt`).

---

### 🇺🇸 English
- 🎨 **Panel & Dock Icon Fixed:** SVG icons (standard and symbolic) are now properly bundled inside `.deb`, `.rpm`, and `.tar.gz` packages. The applet icon now displays correctly in COSMIC Settings, Dock, and Panel.
- 🛡️ **Startup Crash Fixed (`ChecksumMismatch`):** The applet no longer panics during launch when checking FFmpeg. It automatically links to system FFmpeg/FFprobe with fault-tolerant download fallback.
- 🎬 **YouTube Downloads (Video & Audio) Restored:**
  - Bypassed YouTube's SABR-only restrictions using `default,web_embedded,ios` clients.
  - Fixed FFmpeg directory discovery and `ffprobe` detection for embedding thumbnails and album art without postprocessing errors.
  - Automatic download and integrity validation for the latest official `yt-dlp` binary.
- 📦 **Native FFmpeg Dependency in DEB:** Added `ffmpeg` to package dependencies for automatic installation via `apt`.

---

### 🇪🇸 Español
- 🎨 **Icono en Panel y Dock Corregido:** Los iconos SVG ahora se empaquetan correctamente en `.deb`, `.rpm` y `.tar.gz`.
- 🛡️ **Solución al Cierre Inesperado (`ChecksumMismatch`):** Detección automática de FFmpeg del sistema y tolerancia a fallos.
- 🎬 **Descargas de YouTube (Video y Música) Restauradas:**
  - Evita el bloqueo SABR de YouTube con clientes `default,web_embedded,ios`.
  - Detección de `ffprobe` para incrustación de carátulas sin errores de postprocesamiento.
  - Actualización automática al último motor oficial de `yt-dlp`.
- 📦 **Dependencia Nativa de FFmpeg:** Añadido `ffmpeg` a las dependencias de instalación en `.deb`.
