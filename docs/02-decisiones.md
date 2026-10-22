# Decisiones

Formato: decisión, motivo, alternativas descartadas. Principio general: **simple y
duradero**; ninguna funcionalidad sin necesidad real.

## D1. Rust como lenguaje
- **Motivo:** binario nativo por plataforma sin runtime, compatibilidad hacia atrás
  del lenguaje (*editions*) y pocas dependencias.
- **Descartado:** Python (depende de intérprete y de ruedas binarias de terceros que
  se rompen con versiones nuevas), C++/Qt (mantenimiento y toolchain más pesados).

## D2. Núcleo separado de la interfaz
- `jamman-core`: protocolo, formatos, sin GUI. `jamman-cli` y la futura GUI son capas
  finas. Si la GUI queda obsoleta, solo se reescribe esa capa.

## D3. GUI con egui, no Tauri
- **Motivo:** un único binario, un solo lenguaje, sin webview del sistema ni
  `node_modules` que mantener. La herramienta es una utilidad sencilla.
- **Descartado:** Tauri (depende de WebView2/WebKitGTK y del ecosistema npm).
- La GUI **no se crea todavía**: primero el protocolo y la CLI.

## D4. Dependencias mínimas
- De momento `jamman-core` no tiene ninguna. El error se escribe a mano.
- Previsto: un crate de puerto serie (`serialport`) cuando haya que hablar con el pedal.

## D5. Reimplementar, no reutilizar código
- La ingeniería inversa solo sirve para **entender** el protocolo. El código Rust se
  escribe desde cero y se documenta con lo observado.
- El material original (`.exe`, `extracted/`, manual) está en `.gitignore` y no se
  redistribuye.

## D6. Herramientas de análisis
- Ghidra + REA (MCP) en lugar de IDA/Hopper: gratuito y ya probado en Linux.
- El pedal no es imprescindible para el análisis estático; se conectará solo para
  validar el protocolo.

## D7. Configuración de Rust
- `edition = "2024"`, `resolver = "3"`, `rust-version = "1.90"`.
- `unsafe_code = "forbid"`, `missing_docs = "warn"` y `clippy::all` como aviso.
- Se actualizó la toolchain de 1.93 a 1.99.1 con `rustup update stable`.

## D8. Cliente genérico sobre `Read + Write`
- `Client<T: Read + Write>` no conoce el puerto serie. Se prueba con un
  transporte en memoria y se puede validar el protocolo entero sin pedal.
- El puerto real (crate `serialport`) se añade solo en la CLI.

## D9. Lo no confirmado se documenta, no se oculta
- Cada campo cuyo significado es una suposición lleva "hipótesis" en su
  documentación y en `docs/03-protocolo.md`. Se prefiere un nombre neutro
  (`first`, `second`, `flag`) a uno engañoso.
- Los tests fijan lo **observado** (tamaños de carga, IDs, orden de campos).

## D10. Sin parseo de argumentos externo en la CLI
- La CLI tiene pocos subcomandos; se usa `std::env::args`. Se reconsiderará
  `clap` si crece.

## D11. Firmware fuera de alcance
- Escribir firmware puede inutilizar el pedal. No se implementa.
