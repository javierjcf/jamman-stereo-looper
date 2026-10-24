# Proceso seguido

Objetivo: sustituir el software original **JamManager XT** (Windows/Mac, abandonado)
por un programa propio, multiplataforma y de larga duración, para gestionar los
bucles del pedal DigiTech JamMan Stereo.

Entorno: CachyOS (Arch), Node 24, Java 27, Rust estable.

## 1. Reconocimiento
1. `file` sobre `JamManagerXTSetup.exe`: PE32 i386, GUI, 13 MB.
2. `strings` + búsqueda de firmas: es un instalador **Inno Setup 5.5.0**.
3. Del manual (`pdftotext`): bucles = WAV 44,1 kHz/16 bit/estéreo, 99 posiciones
   internas + 99 en SD, USB 2.0 High Speed.

## 2. Extracción del instalador (sin ejecutarlo)
```bash
sudo pacman -S innoextract
innoextract -d extracted JamManagerXTSetup.exe
```
Resultado en `extracted/app/`: `JamManagerXT.exe`, `HwInterface.dll`,
`AudioProcessing.dll`, DLL de Qt 4, `driver/JamMan.inf`, etc.

## 3. Lectura del driver (`JamMan.inf`)
El pedal es `USB\VID_1210&PID_0020` y usa `usbser.sys`: es un **puerto serie
virtual USB (CDC-ACM)**. En Linux lo gestiona el módulo `cdc_acm` del kernel
(aparece como `/dev/ttyACM*`); no hace falta driver propio.

## 4. Herramientas de ingeniería inversa
```bash
sudo pacman -S ghidra                       # Ghidra 12.1.2 en /opt/ghidra
npx rea-agents setup --client claude_code --yes   # servidor MCP + skill de REA
```
- REA lanza Ghidra en modo *headless*; **no hace falta abrir Ghidra a mano**.
- El setup modifica `~/.claude.json` (con copia en `~/.claude.json.rea.backup`).
- Se limitó el setup a Claude Code (REA detecta 9 clientes; solo se tocó uno).

## 5. Análisis estático
1. `HwInterface.dll`: sin exportaciones, solo usa matemáticas y memoria. No
   contiene el protocolo (descartada).
2. `AudioProcessing.dll`: exporta `ConvertAudioFile`, `CopyAndAppendAudioFile`,
   `LevelAudioFile` (conversión, concatenación y nivelado de audio).
3. `JamManagerXT.exe` contiene todo el protocolo. Está hecho con **Qt 4 + JUCE**,
   con hilos `SerialInThread`/`SerialOutThread`.

Técnica seguida con REA (servidor MCP sobre Ghidra) en `JamManagerXT.exe`:

1. **Cadenas → funciones.** `trace_feature "serial"` localiza las cadenas del
   protocolo y las funciones que las usan ("sync receive serial number didn't
   match", "Timeout on serial #", "failed to open remote file"…).
2. **Parámetros del puerto.** Los llamantes de `SetCommState`/`SetupComm`
   (`FUN_00549170`, `FUN_00549070`, `FUN_004799c0`) dan 115200 8N1, handshake
   hardware, búferes de 8 KB y timeout de 10 s.
3. **Comandos de alto nivel.** Los llamantes de la función de envío
   `FUN_004804f0` forman un bloque contiguo (`0x4806f0..0x481a80`): abrir,
   cerrar, leer, listar, borrar, crear, información, identificación…
4. **Clases de mensaje.** Cada comando construye un objeto
   `XxxRequest::vftable`. Leyendo el bloque de tablas virtuales con `read_bytes`
   (`0x574a80`) y decodificándolo con un script se obtienen los escritores de
   carga de cada tipo (`0x47cd00..0x47d260`). Los constructores dan los IDs
   (`0x47ce50` → lectura = 12, `0x47cee0` → escritura = 14).
5. **Cabecera.** `FUN_0047cc50` escribe `u16 tipo`, `u32 longitud`, `u32 serie`
   y las aserciones `itAllAddsUp` / `bytesWritten == _numBytesInHeader` confirman
   que son 10 bytes.
6. **Respuestas.** `FUN_00480270` despacha por tipo de respuesta y cada
   constructor (`0x47da50`, `0x47db60`, `0x47de70`, `0x47fc50`, …) lee los
   campos en orden.

Todo está recogido, con su grado de certeza, en [03-protocolo.md](03-protocolo.md).

## 6. Esqueleto del proyecto Rust y desarrollo con TDD
Workspace con `jamman-core` (librería) y `jamman-cli` (binario `jamman`).
```bash
cargo fmt && cargo clippy --all-targets && cargo test
```
Orden de desarrollo en `jamman-core::protocol` (un commit por paso, tests antes
o junto con el código): primitivas del cable → cabecera → peticiones →
respuestas → cliente síncrono sobre `Read + Write`.

## 7. Tarjeta SD, conversión de audio e interfaz gráfica
1. Se analizó una tarjeta real (61 bucles): estructura, los dos estilos de XML
   (software de PC y pedal), formato de los WAV. Ver [04-tarjeta-sd.md](04-tarjeta-sd.md).
2. Se extrajeron del binario los valores de ritmo, modo de parada, tempo y
   géneros, y del manual su significado.
3. `jamman-core::card`: leer, importar, editar, mover, exportar y borrar, con
   ficheros generados byte a byte iguales a los del software original.
4. `jamman-audio`: conversión con `symphonia` + `rubato`, verificada contra
   `ffmpeg` como medidor independiente.
5. `jamman-gui`: interfaz con egui. Se revisó con capturas de la propia app
   (`JAMMAN_SCREENSHOT`), lo que permitió detectar y corregir la lentitud de la
   forma de onda en una SD lenta y el ancho de la tabla.

## 8. Validación con el pedal e integración (24/10/2026)
1. Permiso de acceso al puerto con una regla udev (`packaging/linux/70-jamman.rules`).
2. Primeras pruebas con la CLI: el `WhoAmI` falló por la longitud de la
   cabecera (incluye el número de serie). Con un volcado en crudo se corrigió.
3. Pruebas aisladas en `/mnt/sdfs/jamman-test` (borrada después): listar, leer
   25 MB íntegros, crear carpeta, escribir varios bloques y borrar. Formatos
   confirmados en [03-protocolo.md](03-protocolo.md), sección 0.
4. `FakePedal` (pedal simulado con esos formatos) para hacer TDD del resto sin
   arriesgar el pedal.
5. `jamman-core::pedal::PedalStore`: los bucles por USB con el mismo modelo que
   la tarjeta. CLI `jamman pedal …` probada con el pedal: listar 62 bucles
   (5 s), añadir un MP3 y exportarlo idéntico, copia de seguridad.
6. GUI: el pedal como un destino más del selector, con las operaciones en
   segundo plano. Comprobada con capturas contra el pedal real.

## Pendiente
- Probar en el pedal restaurar, borrar y editar ajustes por USB.
- Mensajes asíncronos y propiedades.
