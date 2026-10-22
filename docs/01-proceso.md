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
4. Se buscaron cadenas relevantes y se decompilaron las funciones que las usan
   (ver [03-protocolo.md](03-protocolo.md)).

## 6. Esqueleto del proyecto Rust
Workspace en `jamman/` con `jamman-core` (librería) y `jamman-cli` (binario `jamman`).
```bash
cd jamman && cargo fmt && cargo clippy --all-targets && cargo test
```

## Pendiente
- Completar el protocolo (apertura de puerto, formato de mensajes, comandos de
  fichero remoto, estructura de carpetas del pedal).
- Validar con el pedal conectado (`/dev/ttyACM0`).
- Implementar cliente serie en `jamman-core` y comandos en la CLI.
- GUI (egui) cuando el núcleo funcione.
