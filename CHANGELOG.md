# Cambios

## v0.1.0 (24/10/2026)

Primera versión. Binarios para Linux x86_64; Windows aún sin probar.

### Tarjeta SD en el PC (sin pedal)
- Detecta la tarjeta del JamMan y avisa si tiene un formato que el pedal no lee.
- Lista los bucles con sus ajustes; ordenar y buscar.
- Añadir canciones (WAV, MP3, FLAC, OGG, AIFF, M4A) con conversión automática
  al formato del pedal, eligiendo desde qué posición se copian.
- Escuchar con forma de onda, editar ajustes, mover o intercambiar (también
  arrastrando), exportar y borrar varios a la vez.
- Copia de seguridad y restauración.
- Comprueba tras copiar que lo escrito está de verdad en la tarjeta.

### Pedal por USB
- Memoria interna y tarjeta puesta en el pedal: listar, añadir, exportar,
  borrar y editar ajustes.
- Espacio libre del pedal; no sube lo que no cabe.
- Copia de seguridad y restauración.

### Interfaces
- `jamman-gui`: interfaz gráfica con explorador de ficheros al estilo SFTP.
- `jamman`: línea de comandos (`jamman card …`, `jamman pedal …`).

En el paquete va la regla udev para usar el pedal sin ser root (ver README).
