# jamman-stereo-looper

Herramienta libre y multiplataforma (Linux y Windows) para gestionar los bucles
del pedal **DigiTech JamMan Stereo**, pensada para sustituir al software
original (JamManager XT) cuando deje de funcionar.

> **Estado: en desarrollo.** El protocolo USB se ha deducido por ingeniería
> inversa del software original y **aún no se ha validado con el pedal**. Ver
> [docs/03-protocolo.md](docs/03-protocolo.md).

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

## Uso (experimental)

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

## Aviso legal

Proyecto independiente, sin relación con DigiTech ni Harman. El protocolo se ha
estudiado para lograr la interoperabilidad con hardware que el usuario posee. No
se incluye ni se redistribuye código ni material de DigiTech.
