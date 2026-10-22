# jamman-stereo-looper

Herramienta libre y multiplataforma (Linux y Windows) para gestionar los bucles
del pedal **DigiTech JamMan Stereo**, pensada para sustituir al software
original (JamManager XT) cuando deje de funcionar.

![Interfaz gráfica](docs/img/gui.png)

> **Estado: en desarrollo.** La gestión de la tarjeta SD (interfaz gráfica y
> línea de comandos) funciona y está probada con una tarjeta real. El protocolo USB del pedal se ha deducido por ingeniería
> inversa y **aún no se ha validado con el pedal**
> ([docs/03-protocolo.md](docs/03-protocolo.md)).

## Estructura

```
crates/
  jamman-core/   Librería: tarjeta SD, protocolo USB y límites del dispositivo
  jamman-audio/  Conversión de audio al formato del pedal y forma de onda
  jamman-cli/    Binario `jamman` (línea de comandos)
  jamman-gui/    Binario `jamman-gui` (interfaz gráfica, egui)
docs/            Proceso seguido, decisiones y protocolo
```

## Compilar y probar

Requiere Rust 1.95 o superior. En Linux hacen falta las cabeceras de ALSA y
libudev (en Arch: `alsa-lib` y `systemd-libs`; en Debian/Ubuntu:
`libasound2-dev` y `libudev-dev`).

```bash
cargo build --release
cargo test
cargo clippy --all-targets
```

## Interfaz gráfica

```bash
cargo run --release -p jamman-gui                 # detecta la tarjeta sola
cargo run --release -p jamman-gui -- /ruta/a/la/tarjeta
```

- Inserta la tarjeta SD del pedal en el ordenador: si solo hay una, se abre sola.
- **Añadir canciones**: botón «＋ Añadir canciones…» o arrastra ficheros o carpetas
  a la ventana. MP3, FLAC, OGG, AIFF, M4A y WAV se convierten automáticamente al
  formato del pedal (opción de normalizar el volumen).
- **Escuchar**: doble clic en una fila, barra espaciadora o el botón ▶; clic en la
  forma de onda para saltar.
- **Editar** en el panel derecho: nombre, tempo, compás, bucle o una vez,
  invertido, modo de parada, ritmo y catálogo. «Guardar» (Ctrl+S) escribe en la
  tarjeta.
- **Mover/intercambiar** posiciones, **exportar** a WAV y **borrar** (Supr, con
  confirmación).

## Línea de comandos con la tarjeta SD (no necesita el pedal)

Es la función principal: gestionar los bucles directamente en la tarjeta.

```bash
jamman card ls /run/media/$USER/TARJETA                 # lista los bucles
jamman card add /run/media/$USER/TARJETA cancion.mp3    # añade (primera posición libre)
jamman card add /run/media/$USER/TARJETA cancion.wav --slot 30 --name "Mi canción"
jamman card rename /run/media/$USER/TARJETA 30 "Otro nombre"
jamman card export /run/media/$USER/TARJETA 30 copia.wav
jamman card rm /run/media/$USER/TARJETA 30              # pide confirmación
```

El audio se convierte solo a WAV 44,1 kHz, 16 bit, estéreo (máximo 10 minutos
por bucle); `--normalize` ajusta el volumen. Expulsa la tarjeta antes de sacarla. Formato completo en [docs/04-tarjeta-sd.md](docs/04-tarjeta-sd.md).

## Uso con el pedal por USB (experimental)

```bash
cargo run -p jamman-cli -- ports          # busca el pedal (USB 1210:0020)
cargo run -p jamman-cli -- who-am-i       # identificación
cargo run -p jamman-cli -- ls /           # lista una carpeta del pedal
cargo run -p jamman-cli -- get <ruta> <destino>
```

En Linux el pedal aparece como `/dev/ttyACM*`. Tu usuario necesita permiso sobre
ese dispositivo (grupo `uucp` en Arch, `dialout` en Debian/Ubuntu).

## Documentación

- [Proceso seguido](docs/01-proceso.md)
- [Decisiones de diseño](docs/02-decisiones.md)
- [Protocolo del pedal](docs/03-protocolo.md)
- [Formato de la tarjeta SD](docs/04-tarjeta-sd.md)

## Aviso legal

Proyecto independiente, sin relación con DigiTech ni Harman. El protocolo se ha
estudiado para lograr la interoperabilidad con hardware que el usuario posee. No
se incluye ni se redistribuye código ni material de DigiTech.
