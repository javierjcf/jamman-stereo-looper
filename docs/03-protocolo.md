# Protocolo del JamMan Stereo (en construcción)

Fuente: análisis estático de `JamManagerXT.exe` (SHA-256
`dbcd5f94…ec87b`) con Ghidra 12.1.2. **Todo lo marcado como _hipótesis_ está sin
validar con el pedal.**

## Transporte (confirmado por el `.inf`)
- USB `VID 0x1210 / PID 0x0020`, clase CDC-ACM (`usbser.sys`).
- En Linux: `/dev/ttyACM*`. Parámetros de línea (baudios, etc.): **pendiente**
  (la app llama a `SetCommState`).

## Capa de mensajes
La app (JUCE) usa dos hilos, `SerialInThread` y `SerialOutThread`.

Función de envío/respuesta síncrona en `0x004804f0`:
1. Si no hay conexión: "Not connected." / "Send Failed.".
2. Serializa el mensaje y lo envía.
3. Espera respuesta con `0x004803f0` (timeout → "Timeout on serial # …").
4. Comprueba que el **número de serie** de la respuesta coincide con el de la
   petición; si no, lo descarta ("sync receive serial number didn't match").
5. Respuesta con campo de tipo `3` en offset +8 → se trata como caso especial
   (_hipótesis_: respuesta de error o ack).

Layout parcial del mensaje (_hipótesis_ a partir del decompilado):
| Offset | Campo |
|---|---|
| +0 | puntero a tabla virtual (solo en memoria; no va por el cable) |
| +4 | número de serie (secuencia) |
| +8 | tipo de mensaje (`u16`) |

El formato en el cable (cabecera, longitud, checksum) **aún no está identificado**.

## Transferencia de ficheros remotos
Cadenas que indican un sistema de ficheros accesible por el enlace serie:
`failed to open remote file`, `failed to close remote file`,
`Getting remote file`, `Can't get info for remote folder`,
`File transfer completed successfully %lld bytes in %2.2f seconds - %2.2f MBit/s`,
`_path.size() <= 0xFF` (rutas de hasta 255 bytes).

## Organización de los bucles en el pedal
- Dos almacenes: `internal` y `external` (SD). Hasta 99 bucles en cada uno.
- Cada bucle incluye `/PhraseA/phrase.wav` (audio) y metadatos XML con esquemas
  `http://schemas.digitech.com/JamMan/Patch` y `.../Phrase`.
- Metadatos conocidos (columnas de la BD local): nombre, longitud, valoración,
  descripción, artista, género, etiquetas, `StopMode`, `RhythmType`, compases/minuto,
  `isReversed`, `isLoop`, versiones de ajustes y audio, GUIDs, checksums.

## Firmware
Actualización vía `http://digitech.com/updater/common/config/UpdateTargets.xml`
(descarga y transferencia al dispositivo). **Fuera de alcance**: no se implementará
(riesgo de dejar el pedal inservible).

## Próximos pasos
1. Localizar el constructor/serializador de mensajes y la tabla de tipos.
2. Localizar la apertura del puerto (`SetCommState`) para los parámetros de línea.
3. Identificar los mensajes de: info del dispositivo, listar carpeta, abrir/leer/
   escribir/borrar fichero.
4. Validar con el pedal.
