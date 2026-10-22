# jamman-stereo-looper

Herramienta libre y multiplataforma (Linux y Windows) para gestionar los bucles
del pedal **DigiTech JamMan Stereo**, pensada para sustituir al software
original (JamManager XT) cuando deje de funcionar.

**No hace falta conectar el pedal**: basta con meter su tarjeta SD en el
ordenador. Las canciones se añaden, escuchan, editan y exportan directamente en
la tarjeta.

![Interfaz gráfica: explorador del equipo, tarjeta y ajustes del bucle](docs/img/gui.png)

> **Estado: en desarrollo.** La gestión de la tarjeta SD (interfaz gráfica y
> línea de comandos) funciona y está probada con una tarjeta real. La conexión
> con el pedal por USB se ha deducido por ingeniería inversa y **aún no se ha
> validado con el pedal** ([docs/03-protocolo.md](docs/03-protocolo.md)).

## Funciones

### Tarjeta SD
- Detecta sola la tarjeta del JamMan al conectarla (o se abre cualquier carpeta).
- Muestra las posiciones ocupadas (de 99), el espacio libre y cuántos minutos de
  audio caben todavía.
- Lista los bucles con número, nombre, duración, tempo, bucle o una vez, ritmo,
  modo de parada y valoración. Se puede ordenar por columnas y buscar por
  nombre, artista, etiquetas o número.

### Explorador del equipo, como un cliente SFTP
- A la izquierda, las carpetas y el audio compatible del ordenador; a la
  derecha, la tarjeta.
- **Del equipo a la tarjeta**: selecciona canciones o carpetas y pulsa «➡ Añadir a
  la tarjeta», haz doble clic en una canción, usa «＋ Añadir canciones…» o
  arrastra ficheros a la ventana.
- **De la tarjeta al equipo**: selecciona bucles y pulsa «⬅ Exportar a la
  carpeta» para copiar sus WAV a la carpeta abierta en el explorador. Nunca se
  sobrescribe nada (se añade «(2)», «(3)»…).
- **Selección múltiple** en ambos lados: Ctrl+clic, Mayús+clic y Ctrl+A.
- Las transferencias van en segundo plano con barra de progreso; al terminar se
  resumen los errores, si los hay.

### Conversión de audio automática
- MP3, FLAC, OGG, AIFF, M4A y WAV de cualquier frecuencia, resolución y número de
  canales se convierten al formato del pedal: WAV 44,1 kHz, 16 bit, estéreo.
- Normalización de volumen opcional (pico justo por debajo de 0 dB).
- Máximo 10 minutos por bucle, como el pedal.

### Escuchar
- Reproductor con forma de onda: doble clic en un bucle, barra espaciadora o ▶.
  Clic en la forma de onda para saltar a ese punto. Control de volumen.

### Editar cada bucle
Con los ajustes que describe el manual del pedal:
- Nombre, tempo (40–240 BPM), compás (2–15 tiempos), bucle o una vez,
  reproducción invertida.
- Modo de parada (Stop, Finish, Fade) y ritmo guía (oF, r1–r9).
- Catálogo: artista, género, valoración, etiquetas y descripción.
- «Guardar» (Ctrl+S) escribe solo lo que ha cambiado; avisa si cambias de bucle
  con cambios sin guardar.
- Mover un bucle a otra posición (si está ocupada, se intercambian).
- Borrar uno o varios bucles (Supr), con confirmación.

### Seguridad de la tarjeta
- Los ficheros generados son idénticos byte a byte a los del software original.
- Un bucle nuevo se construye aparte y solo aparece al final: un fallo (audio
  inválido, tarjeta llena) no deja nada a medias.
- Al editar se conserva todo lo que no se toca, incluidos los bucles grabados en
  el propio pedal.
- Recuerda **expulsar la tarjeta** antes de sacarla del ordenador.

### Otros
- Tema oscuro, claro o el del sistema.
- Línea de comandos para todo lo de la tarjeta (ver abajo).

## Instalación

Requiere Rust 1.95 o superior. En Linux hacen falta las cabeceras de ALSA y
libudev (en Arch: `alsa-lib` y `systemd-libs`; en Debian/Ubuntu:
`libasound2-dev` y `libudev-dev`).

```bash
git clone https://github.com/javierjcf/jamman-stereo-looper.git
cd jamman-stereo-looper
cargo run --release -p jamman-gui
```

Para tener los comandos `jamman-gui` y `jamman` en el sistema:

```bash
cargo install --path crates/jamman-gui
cargo install --path crates/jamman-cli
```

## Interfaz gráfica

```bash
jamman-gui                      # abre la tarjeta si solo hay una conectada
jamman-gui /ruta/a/la/tarjeta   # abre esa tarjeta (o su carpeta JamManStereo)
```

| Atajo | Acción |
|---|---|
| Clic / Ctrl+clic / Mayús+clic | Seleccionar uno / añadir o quitar / un rango |
| Ctrl+A | Seleccionar todos los bucles visibles |
| Doble clic | Bucle: escuchar · Canción del explorador: añadirla · Carpeta: entrar |
| Espacio | Escuchar o pausar |
| Supr | Borrar la selección (pide confirmación) |
| Ctrl+S | Guardar los ajustes del bucle |

## Línea de comandos

```bash
jamman card ls <tarjeta>                                 # lista los bucles
jamman card add <tarjeta> cancion.mp3                    # añade en la primera posición libre
jamman card add <tarjeta> cancion.flac --slot 30 --name "Mi canción" --normalize
jamman card rename <tarjeta> 30 "Otro nombre"
jamman card export <tarjeta> 30 copia.wav
jamman card rm <tarjeta> 30                              # pide confirmación (o --yes)
jamman card init <punto-de-montaje>                      # prepara una tarjeta vacía
```

`<tarjeta>` es el punto de montaje (en Linux, p. ej. `/run/media/$USER/ETIQUETA`;
en Windows, `E:\`). Sin instalar, usa `cargo run --release -p jamman-cli -- card …`.

### Pedal por USB (experimental, sin validar)

```bash
jamman ports              # busca el pedal (USB 1210:0020)
jamman who-am-i           # identificación
jamman ls /               # lista una carpeta del pedal
jamman get <ruta> <destino>
```

En Linux el pedal aparece como `/dev/ttyACM*`; tu usuario necesita permiso sobre
ese dispositivo (grupo `uucp` en Arch, `dialout` en Debian/Ubuntu).

## Qué falta

Revisado contra el código y contra el análisis del programa original
(JamManager XT). Tres niveles: lo **imprescindible** para fiarse de la carga de
canciones, lo **importante** para el buen funcionamiento y la paridad con el
original, y lo **opcional**.

> Las filas marcadas con 🔍 dependen de un análisis del original que está en
> curso; se actualizarán con lo que se encuentre.

### 🔴 Imprescindible (que cargar en la SD sea fiable)

| # | Qué falta | Por qué importa |
|---|---|---|
| 1 | **Comprobar en el pedal un bucle escrito por la app.** | Todo el formato se ha contrastado con bucles escritos por el software original, pero no consta que el pedal haya cargado uno generado por esta app. Prueba mínima: meter la tarjeta en el pedal y comprobar que el bucle aparece en su posición, suena a su velocidad y respeta tempo, compás, parada y ritmo. |
| 2 | **Escritura segura en FAT.** Solo el audio se sincroniza (`sync_all`); `patch.xml`, `phrase.xml` y el renombrado de la carpeta no. La GUI solo *recuerda* expulsar la tarjeta. | Si se saca la tarjeta justo después de importar, puede quedar un bucle sin XML o sin carpeta. Hace falta sincronizar ficheros y carpeta al terminar, y un botón de **expulsión segura** (Linux: `udisks`; Windows: expulsar volumen). |
| 3 | **Comprobar el espacio antes de copiar** y **limpiar restos** `.tmp-PatchNN`. | Hoy la tarjeta llena se detecta al fallar la copia (no queda nada a medias, pero se pierde tiempo copiando decenas de MB). El original avisa antes («not enough space on external storage…»). Un corte de corriente deja carpetas temporales que no se limpian al abrir la tarjeta. |
| 4 | 🔍 **Tarjetas nuevas o recién formateadas.** | La GUI solo detecta tarjetas que ya tienen la carpeta `JamManStereo`; con una tarjeta vacía hay que usar `jamman card init` (no existe en la GUI). Falta saber qué hace el original si la carpeta no existe y si el pedal crea algo más al formatear. Además la tarjeta debe ser FAT32/SDHC (hasta 32 GB); exFAT no la lee el pedal: conviene avisarlo. |
| 5 | 🔍 **`SettingsVersion` y `AudioVersion`.** | Al editar un bucle o cambiar su audio la app no toca estos contadores. El original los usa para saber si un bucle cambió; no se sabe si el pedal depende de ellos. |
| 6 | **Windows: compilación y pruebas.** No hay integración continua y no consta que la GUI se haya compilado y probado en Windows (detección de unidades `A:`–`Z:`, rutas, audio). | Es un objetivo del proyecto. Falta un flujo de GitHub Actions para Linux y Windows y una prueba manual con una tarjeta real. |

### 🟡 Importante (buen funcionamiento y paridad con el original)

| Qué falta | Notas |
|---|---|
| **Multiplicar un bucle** (repetirlo *n* veces) | El original lo hace sobre el bucle ya cargado, avisa si no hay espacio y rechaza lo que pase de 10 minutos. |
| **Atenuar 3 dB / 6 dB y normalizar** un bucle ya en la tarjeta | Hoy solo se normaliza al importar. |
| **Aviso al quedarse sin posiciones libres** al importar varios ficheros | El original avisa de que «solo se copiarán los bucles que no pasen del último hueco». Comprobar cómo se comporta la GUI. |
| **Nombres duplicados** | El original renombra el duplicado. En la tarjeta el pedal solo muestra el número, pero la búsqueda de la GUI sí se resiente. |
| **Tarjeta bloqueada** (interruptor de solo lectura) y tarjeta retirada en caliente | Mostrar un mensaje claro en lugar de un error de E/S. |
| **Deshacer y rehacer** | El original lo tiene en su biblioteca y en las opciones de cada bucle (clases `QUndoCommand`). |
| 🔍 **Tempo y compás por defecto de un bucle importado** | Hoy se escribe 120 BPM y 4 tiempos. Falta ver si el original hace algo más y si el pedal sincroniza mejor con otro valor. |

### 🟢 Opcional

| Qué falta | Notas |
|---|---|
| **Pedal por USB** (memoria interna, leer y escribir bucles) | El protocolo está implementado pero **sin validar con el pedal**. No hace falta para trabajar con la tarjeta; solo para la memoria interna. Pendientes: raíz del sistema de ficheros, modo de apertura para escribir, mensajes asíncronos y propiedades (ver [docs/03-protocolo.md](docs/03-protocolo.md)). |
| **Biblioteca local** (JamLists, valoración y búsqueda persistentes) y diálogo de **sincronización** con la tarjeta | El original guarda una base SQLite. El explorador del equipo cubre lo básico. |
| **Detección automática de tempo** | El pedal mide el tempo al grabar (`BpmValidated` en `phrase.xml`). Si el original hace algo parecido al importar está por confirmar (🔍). |
| **Idioma y preferencias**, **instaladores** (AppImage, `.deb`, `.msi`) | |
| **Actualización de firmware** | Descartada a propósito: un fallo puede inutilizar el pedal. |

## Desarrollo

```bash
cargo test                     # tests de todo el proyecto
cargo clippy --all-targets     # sin avisos
```

```
crates/
  jamman-core/   Librería: tarjeta SD, protocolo USB y límites del dispositivo
  jamman-audio/  Conversión de audio al formato del pedal y forma de onda
  jamman-cli/    Binario `jamman` (línea de comandos)
  jamman-gui/    Binario `jamman-gui` (interfaz gráfica con egui)
docs/            Proceso seguido, decisiones, protocolo y formato de la tarjeta
```

La lógica de la interfaz (`state.rs`, `browser.rs`, `selection.rs`) no depende
de egui y tiene tests. `JAMMAN_SCREENSHOT=captura.ppm jamman-gui` guarda una
captura de la ventana (ver `crates/jamman-gui/src/devshot.rs`).

## Documentación

- [Proceso seguido](docs/01-proceso.md)
- [Decisiones de diseño](docs/02-decisiones.md)
- [Protocolo del pedal](docs/03-protocolo.md)
- [Formato de la tarjeta SD](docs/04-tarjeta-sd.md)

## Aviso legal

Proyecto independiente, sin relación con DigiTech ni Harman. El protocolo se ha
estudiado para lograr la interoperabilidad con hardware que el usuario posee. No
se incluye ni se redistribuye código ni material de DigiTech.
