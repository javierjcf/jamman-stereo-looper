# jamman-stereo-looper

Herramienta libre y multiplataforma (Linux y Windows) para gestionar los bucles
del pedal **DigiTech JamMan Stereo**, pensada para sustituir al software
original (JamManager XT) cuando deje de funcionar.

> **Estado: en desarrollo.** La gestión de la tarjeta SD funciona y está probada
> con una tarjeta real. El protocolo USB del pedal se ha deducido por ingeniería
> inversa y **aún no se ha validado con el pedal**
> ([docs/03-protocolo.md](docs/03-protocolo.md)).

## Estructura

```
crates/
  jamman-core/   Librería: protocolo, formatos y límites del dispositivo (sin GUI)
  jamman-cli/    Binario `jamman` (línea de comandos)
docs/            Proceso seguido, decisiones y protocolo
```

## Compilar y probar

Requiere Rust estable reciente (ver `rust-version` en `Cargo.toml`).

```bash
cargo build --release
cargo test
cargo clippy --all-targets
```

## Uso con la tarjeta SD (no necesita el pedal)

Es la función principal: gestionar los bucles directamente en la tarjeta.

```bash
jamman card ls /run/media/$USER/TARJETA                 # lista los bucles
jamman card add /run/media/$USER/TARJETA cancion.wav    # añade (primera posición libre)
jamman card add /run/media/$USER/TARJETA cancion.wav --slot 30 --name "Mi canción"
jamman card rename /run/media/$USER/TARJETA 30 "Otro nombre"
jamman card export /run/media/$USER/TARJETA 30 copia.wav
jamman card rm /run/media/$USER/TARJETA 30              # pide confirmación
```

El audio debe ser WAV 44,1 kHz, 16 bit, estéreo (hasta 10 minutos); si no, la
herramienta indica el comando `ffmpeg` para convertirlo. Expulsa la tarjeta antes
de sacarla. Formato completo en [docs/04-tarjeta-sd.md](docs/04-tarjeta-sd.md).

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
