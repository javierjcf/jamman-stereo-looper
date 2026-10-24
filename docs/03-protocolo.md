# Protocolo del JamMan Stereo ("Greenglass")

Fuente: análisis estático de `JamManagerXT.exe` (SHA-256
`dbcd5f94…ec87b`) con Ghidra 12.1.2 a través de REA. Está hecho con **Qt 4 + JUCE**
y el código del protocolo vive en `Logic\HwInterface\GreenglassClient.cpp` y
`GreenglassClientMessages.h` (rutas que aparecen en las aserciones del binario).
"Greenglass" es el nombre interno del protocolo.

**Niveles de certeza** usados en este documento:

- ✅ **Observado**: leído directamente del código decompilado.
- 🔶 **Deducido**: se infiere de forma razonable (por ejemplo por la estructura
  de otras funciones) pero no se ha visto literalmente.
- ❓ **Hipótesis**: suposición. Hay que validarla con el pedal.

El código Rust de [`protocol/`](../crates/jamman-core/src/protocol) implementa
todo lo de este documento y marca en la documentación de cada campo lo que es
hipótesis.

## 0. Validación con el pedal (24/10/2026)

Probado con un JamMan Stereo real conectado por USB a Linux:

| Dato | Resultado |
|---|---|
| Dispositivo `1210:0020` → `/dev/ttyACM0` (`cdc_acm`) | ✅ |
| Cabecera: el campo de longitud **incluye los 4 bytes de la serie** | ✅ corregido (antes se suponía solo la carga) |
| `WhoAmI` → `[93, 3, 1, 4864]`, GUID del aparato y «JamMan Stereo» | ✅ |
| `FolderContents`: primera lista ficheros, segunda carpetas | ✅ |
| `EntryInfo`: el primer byte es el tipo (0 fichero, 1 carpeta); el byte tras el nombre es «existe»; si el tipo no coincide, el pedal responde con error | ✅ |
| Fechas: el mes empieza en 0 | ✅ |
| `FileOpen` lectura `[1,1]`; escritura `[2,0]` crea o vacía el fichero | ✅ |
| `FileRead`/`FileWrite` por bloques de 8169 bytes; 25 MB a ~1,85 MB/s, íntegros | ✅ |
| `EntryCreate` carpeta `[1, ruta, 1]` | ✅ |
| `EntryDelete` `[tipo, ruta, 1, 0]`: borra ficheros y carpetas **con su contenido** | ✅ |
| Mensajes asíncronos (`0x8006`) aparecen solos; se descartan sin perder la sincronía | ✅ |

**Sistema de ficheros del pedal** (tipo Unix):

```
/mnt/nandfs/JamManStereo/PatchNN/…   memoria interna (NAND)
/mnt/sdfs/JamManStereo/PatchNN/…     tarjeta SD insertada en el pedal
```

La estructura de cada bucle es la misma que en la tarjeta leída desde el PC
(`patch.xml`, `PhraseA/phrase.wav`, `PhraseA/phrase.xml`). Una posición con
ajustes guardados pero sin grabar tiene `phrase.xml` y no `phrase.wav`.

## 1. Transporte

| Dato | Valor | Certeza |
|---|---|---|
| USB | `VID 0x1210`, `PID 0x0020` ("Jam Man USB Device") | ✅ `JamMan.inf` |
| Clase | CDC-ACM (`usbser.sys`), puerto serie virtual | ✅ `JamMan.inf` |
| Linux | `/dev/ttyACM*` por el módulo `cdc_acm` del kernel | 🔶 |
| Línea | 115200 baudios, 8 bits, sin paridad, 1 bit de parada | ✅ `FUN_004799c0` |
| Control de flujo | RTS/CTS + DTR/DSR por hardware | ✅ `FUN_00549170` |
| Búferes | 8192 bytes de entrada y salida (`SetupComm`) | ✅ |
| Timeout de lectura | 10 000 ms (intervalo y multiplicador sin límite) | ✅ `FUN_00549070` |
| Timeout por petición | 1000 ms (`0x3e8`) | ✅ |

En un puerto CDC-ACM los baudios y el control de flujo no influyen en la
velocidad real, pero conviene reproducir 115200 8N1 y mantener DTR activo.

## 2. Marco de mensaje

Todos los enteros son **little-endian** (🔶: JUCE `writeShort/Int/Int64`, que
son los slots 7, 9 y 11 de `OutputStream`, no las variantes `BigEndian`).

```
 0      2          6         10
 +------+----------+---------+------------- - - -
 | tipo | long. de | serie   | carga (long. bytes)
 | u16  | carga u32| u32     |
 +------+----------+---------+------------- - - -
```

- `tipo`: identifica el mensaje (secciones 4 y 5).
- `longitud de carga`: bytes que siguen a la cabecera ✅. El original comprueba
  con la aserción `itAllAddsUp` que lo escrito es `10 + longitud`.
- `serie`: contador global que empieza en 0 y crece de uno en uno ✅ (variable
  `DAT_006aaa98`). La respuesta devuelve el mismo número.
- Una cadena es `u32` con la longitud en bytes más los bytes (sin terminador)
  ✅. La codificación se asume UTF-8 🔶.
- Las rutas tienen un máximo de 255 bytes ✅ (`_path.size() <= 0xFF`).

### Regla de emparejamiento ✅

`FUN_004804f0` envía la petición y espera la respuesta con `FUN_004803f0`. Si el
número de serie recibido no coincide, escribe "sync receive serial number didn't
match! Expected %d, got %d, Discarding." y **descarta** el mensaje. Si no llega
nada en el tiempo fijado, "Timeout on serial # …". Una `FailureResponse`
(tipo 3) es un error del lado del pedal.

### Mensajes asíncronos

Los tipos `0x8000..=0x8006` los inicia el pedal (el original los trata en otro
sitio y aquí registra "Unexepected async message ID"). Su contenido **no está
descifrado** ❓; el cliente de `jamman-core` los ignora.

## 3. Cómo se lee en el código original

| Función | Primitiva |
|---|---|
| `FUN_00474d00` | `u8` |
| `FUN_00474d20` | `u16` |
| `FUN_00474c30` | `u32` |
| `FUN_00474c70` | `i64` |
| `FUN_00474d50` | `i8` |
| `FUN_0047cb70` | cadena (`u32` + bytes) |
| `FUN_0047d320` | fecha: `u16` año + 4 × `i8` |

## 4. Peticiones (PC → pedal)

Cargas (después de los 10 bytes de cabecera). ✅ salvo indicación.

| ID | Mensaje | Carga | Notas |
|---|---|---|---|
| 0 | `WhoAmIRequest` | vacía | |
| 4 | `FileSystemEntryInfoRequest` | `u8` flag, cadena ruta | el flag es un parámetro del llamante ❓ |
| 7 | `FileSystemEntryCreateRequest` | `u8`=1, cadena ruta, `u8`=1 | valores fijos en el original ❓ significado |
| 9 | `FileSystemEntryDeleteRequest` | `u8`=1, cadena ruta, `u8`=1, `u8`=0 | idem |
| 10 | `FileOpenRequest` | cadena ruta, `u8`, `u8` | lectura = `1,1` ✅; escritura = `2,0` ❓ |
| 12 | `FileReadRequest` | `i64` manejador, `u32` longitud | máximo 8169 (`0x1fe9`) por mensaje ✅ |
| 14 | `FileWriteRequest` | `i64` manejador, `u32` n, n bytes | máximo 8169 por mensaje ✅ |
| 15 | `FileSeekRequest` | `i64` manejador, `i64` posición, `u8` | el original usa posición 40 y flag 0 para leer la longitud del WAV |
| 16 | `FileCloseRequest` | `i64` manejador | |
| 17 | `FolderContentsRequest` | cadena ruta | |
| 22 | `IssueCommandRequest` | 3 enteros | 🔶 comandos genéricos; **sin implementar** |
| 26 (`0x1a`) | `GetPropertyRequest` | `u8` n, n × `u16` (el original la envía vacía), `u16` id | ✅ validado con el pedal |

Sin descifrar todavía: `0x13` (🔶 petición de propiedades múltiples, respuesta
`0x14`) y `0x17` (lanzar proceso, respuesta `0x18`).

### Propiedades (validado con el pedal, 24/10/2026)

El original las lee antes de copiar para avisar de «not enough space…»
(`FUN_0047b090`, que compara el tamaño del fichero con los MB libres). Son
kilobytes:

| Id | Valor | Ejemplo real |
|---|---|---|
| 10 | KB libres de la memoria interna | 444 170 |
| 11 | KB totales de la memoria interna | 516 608 |
| 12 | KB libres de la tarjeta SD | 13 368 608 |
| 13 | KB totales de la tarjeta SD | 15 432 704 (tarjeta de 16 GB) |

**El pedal no avisa cuando no cabe un fichero**: acepta las escrituras y
responde `Success`. Por eso la app comprueba el espacio antes de subir.

### Incidente: escrituras perdidas en la tarjeta del pedal

Con una tarjeta de 16 GB y ~1,97 GiB ocupados, todo lo que se escribió por USB
a partir de ese punto se perdió en silencio: el fichero quedaba con el tamaño
correcto pero con contenido viejo de la tarjeta, y el pedal llegó a colgarse.
La memoria interna, en las mismas condiciones, funcionaba perfectamente
(probado con `examples/write_verify`). El corte coincide con 2 GiB (2³¹): o la
tarjeta es falsa o está defectuosa, o el pedal falla al escribir más allá de
ese punto. Pendiente de comprobar la tarjeta en el PC con `f3`.

### Observaciones útiles

- La longitud de un WAV se obtiene abriendo `phrase.wav`, haciendo `seek` a la
  posición 40 (campo de tamaño de datos de la cabecera WAV) y leyendo 4 bytes;
  después se calcula la duración como `bytes / (canales × 88200)`. ✅
- La transferencia es por bloques de ≤ 8169 bytes y el original informa de la
  velocidad en MBit/s ("File transfer completed successfully …").

## 5. Respuestas (pedal → PC)

El ID de respuesta suele ser el de la petición + 1 (4→5, 10→11, 12→13, 17→18).

| ID | Mensaje | Carga | Certeza |
|---|---|---|---|
| 1 | `WhoAmIResponse` | `u16`×4, cadena, cadena | ✅ estructura; ❓ significado |
| 2 | `SuccessResponse` | vacía | ✅ |
| 3 | `FailureResponse` | cadena con el mensaje | ✅ |
| 5 | `FileSystemEntryInfoResponse` | `u8`, cadena, `u8`, `i64`, `u8`, `u8`, 3 fechas | ✅ estructura; ❓ significado |
| 11 (`0x0b`) | `FileOpenResponse` | `i64` manejador, `i64` tamaño | ✅ |
| 13 (`0x0d`) | `FileReadResponse` | `i64`, `u32` longitud, datos | ✅ cabecera; 🔶 los datos siguen a la longitud |
| 18 (`0x12`) | `FolderContentsResponse` | `u32` n, n cadenas, `u32` m, m cadenas | ✅ estructura; ❓ cuál lista es de ficheros |
| 20 (`0x14`) | respuesta de propiedades | `u16` n, n×(`u16`,`u32`), `u16` m, m×(`u16`, cadena) | ✅ estructura; sin implementar |
| 24 (`0x18`) | `ProcessLaunchResponse` | `i64` | sin implementar |
| 27 (`0x1b`) | `GetPropertyResponse` | `u16` id, si `id` ∈ `0x8000..=0x8018` cadena, si no `u32` | sin implementar |

Las respuestas a *seek*, *close*, *create*, *delete* y *write* no se han
identificado de forma explícita; el cliente las trata como cualquier respuesta
(en la práctica se espera `SuccessResponse`).

Una fecha son 6 bytes: año `u16` y cuatro `i8` (🔶 mes, día, hora, minuto).

## 6. Organización de los bucles en el pedal

- Dos almacenes: `internal` y `external` (SD) ✅ (columna `storagepath` de la BD
  local). Hasta 99 bucles en cada uno.
- Cada bucle tiene una carpeta con `PhraseA/phrase.wav` ✅ (ruta literal
  `/PhraseA/phrase.wav`) y metadatos XML con los esquemas
  `http://schemas.digitech.com/JamMan/Patch` y `…/Phrase` ✅.
- En la **tarjeta SD** el formato es `JamManStereo/PatchNN/PhraseA/…` (ver
  [04-tarjeta-sd.md](04-tarjeta-sd.md)); es razonable (🔶) que la memoria interna
  del pedal use la misma estructura.
- **La ruta raíz de cada almacén y el nombre de la carpeta de cada bucle aún no
  se conocen** ❓. Se averiguarán listando las carpetas con el pedal conectado.
- Metadatos de la biblioteca local (SQLite): nombre, longitud, valoración,
  descripción, artista, género, etiquetas, `StopMode`, `RhythmType`,
  compases/minuto, `isReversed`, `isLoop`, versiones de ajustes y audio, GUIDs,
  checksums.
- Audio: WAV PCM 44,1 kHz, 16 bit, estéreo; el programa original también acepta
  importar `.wav`, `.aif`, `.aiff` y `.ogg`, y convierte a WAV.
- Un bucle no puede exceder 10 minutos ("the resulting file length would exceed
  10 minutes").

## 7. Firmware

El original descarga `http://digitech.com/updater/common/config/UpdateTargets.xml`
y transfiere el firmware al pedal. **Fuera de alcance**: un fallo podría dejar
el pedal inservible.

## 8. Qué falta por validar con el pedal

1. Que el pedal aparece como `/dev/ttyACM*` y acepta 115200 8N1.
2. Que `WhoAmI` responde y qué significan los 4 `u16` y las 2 cadenas.
3. Raíz del sistema de ficheros: probar `FolderContents("/")` y similares.
4. Estructura de carpetas de un bucle y del XML de metadatos.
5. Modo de apertura para **escritura** (`2,0` es solo una hipótesis).
6. Significado de los bytes fijos en `EntryCreate`/`EntryDelete` y del flag de
   `EntryInfo`.
7. Respuestas a `FileSeek`, `FileClose`, `FileWrite`, `EntryCreate`, `EntryDelete`.
8. Mensajes asíncronos `0x8000..=0x8006`.
9. Orden real de los bytes (se asume little-endian).

Una forma de resolver varias a la vez es capturar el tráfico del software
original en Windows (por ejemplo con una VM y reenvío USB) y compararlo con lo
que genera `jamman-core`.
