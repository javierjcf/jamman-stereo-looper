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
