# Tarjeta SD del JamMan Stereo

La tarjeta es un sistema de ficheros FAT corriente. **No hace falta el pedal para
leerla ni escribirla**: basta con montarla en el ordenador. Este formato se ha
comprobado en una tarjeta real (SDHC de 16 GB, 61 bucles, 2 GB de audio) y
coincide con lo que el software original (JamManager XT) escribe.

## Estructura

```
<tarjeta>/
└── JamManStereo/
    ├── Patch01/
    │   ├── patch.xml
    │   └── PhraseA/
    │       ├── phrase.wav
    │       └── phrase.xml
    ├── Patch02/ …
    └── Patch99/
```

- Una carpeta `PatchNN` por bucle, con `NN` de `01` a `99`. **Pueden faltar
  números**: la tarjeta de ejemplo usa 01–20, 24, 51–74 y 80–95.
- El número es la posición de bucle que se selecciona en el pedal con el mando
  *Loop Select* cuando hay tarjeta insertada.
- Siempre hay una única frase, `PhraseA`.

## `patch.xml`

Nombre, ritmo y metadatos de biblioteca. Ejemplo (software de PC, CRLF):

```xml
<?xml version="1.0" encoding="UTF-8"?>

<JamManPatch device="JamManStereo" version="1" xmlns="http://schemas.digitech.com/JamMan/Patch">
  <PatchName>01 Nirvana - Smells like teen spirit BT (vocals)</PatchName>
  <RhythmType>Silence</RhythmType>
  <StopMode>StopInstantly</StopMode>
  <SettingsVersion>0</SettingsVersion>
  <ID>337c3976-2dec-4c32-a4b9-46a9d4004d42</ID>
  <OriginID>337c3976-2dec-4c32-a4b9-46a9d4004d42</OriginID>
  <Metadata>
    <Description>Imported from D:/…/01 Nirvana….wav</Description>
    <Artist></Artist>
    <Genre>Unspecified</Genre>
    <Rating>0</Rating>
    <Tags></Tags>
  </Metadata>
</JamManPatch>
```

## `PhraseA/phrase.xml`

```xml
<JamManPhrase xmlns="http://schemas.digitech.com/JamMan/Phrase" version="1">
  <BeatsPerMinute>120</BeatsPerMinute>
  <BeatsPerMeasure>4</BeatsPerMeasure>
  <IsLoop>0</IsLoop>
  <IsReversed>0</IsReversed>
  <SettingsVersion>0</SettingsVersion>
  <AudioVersion>0</AudioVersion>
  <ID>337c3976-2dec-4c32-a4b9-46a9d4004d42</ID>
</JamManPhrase>
```

## `PhraseA/phrase.wav`

PCM sin compresión, **44,1 kHz, 16 bit, estéreo**, hasta 10 minutos. En la
tarjeta de ejemplo los 61 ficheros cumplen esto; 34 llevan un bloque `LIST/INFO`
(de `ffmpeg`) antes de `data` y 27 no, así que los datos **no empiezan siempre en
el byte 44**: hay que recorrer los bloques RIFF.

## Dos estilos de fichero

| | Software de PC | Pedal (grabado en el propio pedal) |
|---|---|---|
| Saltos de línea | CRLF | LF |
| Indentación | 2 espacios | 4 espacios |
| Etiquetas vacías | `<Artist></Artist>` | `<Metadata />` |
| `PatchName` | nombre del fichero | vacío |
| `OriginID` | igual que `ID` | no existe |
| GUID | versión 4 (aleatorio) | versión 1 (`…-11b2-…`) |
| `BeatsPerMinute` | `120` | decimal medido (`51.0638313293`) |
| Extras | — | `BpmValidated` en `phrase.xml`, `IsLoop` = 1 |

`jamman-core` **lee ambos** y **genera el del software de PC** (byte a byte igual,
verificado con los tamaños reales de 721 y 392 bytes), que es el que el pedal ya
lee en la tarjeta de ejemplo. Para ello:

- No se modifica el audio: se copia tal cual.
- Los GUID de los bucles nuevos son v4 aleatorios (`ID` = `OriginID`; el `ID` de
  la frase es el mismo).
- El nombre por defecto es el del fichero sin extensión; la descripción es
  `Imported from <fichero>`.

## Operaciones y garantías

| Operación | Qué hace |
|---|---|
| `list` | Ordenado por posición. Un bucle dañado se informa sin ocultar los demás; carpetas ajenas (`Patch100`, `Otra`…) se ignoran. |
| `import_wav` | Valida el audio, lo copia y escribe los XML en `.tmp-PatchNN`, y solo entonces renombra a `PatchNN`. Si algo falla (audio inválido, tarjeta llena) no queda nada a medias. No pisa posiciones ocupadas. |
| `rename` | Sustituye solo el texto de `<PatchName>`; el resto del fichero queda intacto. |
| `export_wav` | Copia el audio; no sobrescribe. |
| `remove` | Borra la carpeta completa. Irreversible (la CLI pide confirmación). |

| `update` | Guarda nombre, ritmo, parada, tempo, compás, bucle/una vez, inversión y catálogo. Solo cambia las etiquetas afectadas, no añade etiquetas con valor por defecto y conserva las desconocidas. |
| `move_to` / `swap` | Mueve un bucle a una posición libre o intercambia dos; `swap` deshace los pasos si algo falla. |

## Valores de los ajustes

Sacados de las cadenas del software original y del manual:

| Ajuste | Etiqueta | Valores |
|---|---|---|
| Ritmo | `RhythmType` | `Silence` (oF), `WoodBlocks` (r1), `Sticks` (r2), `Click` (r3), `AlternativeKickAndHighHat` (r4), `StudioKickAndHighHat` (r5), `TechnoKickAndHighHat` (r6), `Cowbell` (r7), `Conga` (r8), `Tambourine` (r9) |
| Parada | `StopMode` | `StopInstantly` (Stop), `StopAtEndOfLoop` (Finish), `FadeOut` (Fade) |
| Tempo | `BeatsPerMinute` | 40 a 240, admite decimales |
| Compás | `BeatsPerMeasure` | 2 a 15 |
| Bucle / una vez | `IsLoop` | `1` / `0` |
| Invertido | `IsReversed` | `1` / `0` |
| Género | `Genre` | Unspecified, Alternative, Blues, Country, Folk, Funk, Fusion, Hip Hop, Jazz, Latin, Metal, New Age, Rock, Techno |
| Valoración | `Rating` | 0 a 5 |

## Conversión de audio

`jamman-audio` decodifica MP3, FLAC, OGG/Vorbis, WAV, AIFF y M4A, pasa a estéreo
(un fichero mono se duplica), remuestrea a 44,1 kHz y escribe WAV de 16 bit, con
normalización opcional (pico a -0,2 dBFS). Un MP3 de 5 minutos tarda ~0,6 s.

## Relación con el protocolo USB

El software original usa la ruta `/PhraseA/phrase.wav` al hablar con el pedal, lo
que sugiere (🔶) que **la memoria interna del pedal expone la misma estructura**
de carpetas `PatchNN/PhraseA/…`. La ruta raíz exacta se averiguará con el pedal
conectado (ver [03-protocolo.md](03-protocolo.md)). Si se confirma, la misma
lógica de bucles servirá para tarjeta y memoria interna.
