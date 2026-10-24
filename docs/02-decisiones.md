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

## D12. La tarjeta SD primero
- La función principal es gestionar bucles en la tarjeta, que se monta como un
  disco normal: no requiere el pedal ni el protocolo USB. Se implementó antes de
  validar el USB y se prueba con carpetas temporales.
- Un WAV que ya cumple el formato se copia **sin modificar**; el resto se
  convierte (ver D15).

## D13. Escritura segura en la tarjeta
- Un bucle nuevo se construye en `.tmp-PatchNN` y se renombra al final.
- Los XML se generan idénticos a los del software de PC (CRLF, sin BOM), probado
  con los tamaños reales.
- `rename` reemplaza solo el texto de `<PatchName>`.
- El borrado es irreversible: la CLI confirma salvo `--yes`.

## D14. Dependencias del núcleo
- `uuid` (solo `v4`) para los GUID de bucles nuevos. `tempfile` solo en tests.
- El lector XML es propio y mínimo (los ficheros son planos); ver `card/xml.rs`.

## D15. Conversión de audio en un crate propio (`jamman-audio`)
- `symphonia` (decodificación) y `rubato` (remuestreo FFT): Rust puro, sin
  bibliotecas del sistema, mantenidos y multiplataforma.
- El audio se decodifica entero en memoria: es lo más simple y el máximo (10 min)
  cabe de sobra.
- Separado del núcleo para que `jamman-core` siga siendo ligero.
- Las dependencias y `jamman-audio` se compilan optimizados también en
  desarrollo: sin optimizar, un MP3 de 5 minutos tardaba 17 s.

## D16. Interfaz gráfica con egui
- `state.rs` (tarjeta, filas, filtro, orden, selección, borrador) no depende de
  egui y tiene tests: otra interfaz podría reutilizarlo.
- Los trabajos lentos (importar, forma de onda) van en hilos con canales; la
  interfaz nunca se bloquea.
- La forma de onda se muestrea (32 KB por tramo, 400 tramos) y se guarda en una
  caché en disco: en un lector de SD lento pasa de 10 s a 2,4 s la primera vez y
  es instantánea después.
- Reproducción con `rodio` (solo WAV): el dispositivo de audio se abre la
  primera vez que se reproduce, así que sin salida de audio la app funciona igual.
- `rust-version` sube a 1.95 para usar egui/eframe 0.37, la versión actual.
- `JAMMAN_SCREENSHOT` guarda una captura de la ventana: sirve para revisar el
  diseño y generar la imagen del README sin herramientas externas.

## D17. Explorador del equipo al estilo SFTP
- El panel izquierdo muestra el equipo y el centro la tarjeta: las
  transferencias se hacen con botones de dirección («➡ Añadir», «⬅ Exportar»)
  sobre la selección, como en los clientes SFTP. Sin arrastrar entre paneles,
  para no complicarlo.
- Solo se listan carpetas y audio compatible; los ocultos no.
- La exportación nunca sobrescribe (añade «(2)»…) y sanea los nombres para que
  valgan en Windows y Linux.
- Selección múltiple genérica (`selection.rs`) compartida por las dos listas; el
  bucle "principal" (el último pulsado) es el que se edita y reproduce.
- La ruta se muestra con la carpeta personal abreviada como `~`.

## D18. El pedal como un destino más
- `PedalStore` ofrece por USB lo mismo que `Card` (listar, leer, importar,
  exportar, borrar, editar), reutilizando los mismos XML y ajustes: la
  interfaz no distingue apenas entre tarjeta y pedal.
- El puerto se abre **en cada operación**, en un hilo, y se cierra al acabar:
  la interfaz no se bloquea y si se desenchufa el pedal no queda nada colgado.
  Cada operación termina volviendo a listar, así la tabla siempre refleja el
  pedal.
- Al subir un bucle, `patch.xml` se escribe el último: si algo falla antes, se
  borra la carpeta y el pedal no ve un bucle a medias.
- Por USB no hay reproducción ni mover/intercambiar: obligaría a descargar y
  volver a subir decenas de MB. La copia de seguridad usa el formato de la
  tarjeta para poder abrirla y restaurarla con las mismas herramientas.
- `serialport` es una característica (`serial`) del núcleo para que los tests
  y quien solo use la tarjeta no la necesiten. Los tests usan `FakePedal`
  (característica `testing`), que responde con los formatos validados.
- Permiso en Linux con una regla udev (`uaccess`) en lugar del grupo `uucp`:
  solo el JamMan, solo el usuario de la sesión y sin reiniciar sesión.
