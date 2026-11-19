# Rendimiento

Equipo: Windows 11 Pro 10.0.26200, monitor 1920×1080. Build Release (`lto = "thin"`), commit `86a4062`. Comparación con el checkpoint anterior: `E:/workspace/aeros-precision-build/release/aeros-editor.exe` (`7e0d027`).

## 1. Proceso: CPU, memoria y arranque

Archivo de datos: `performance-samples.json`.

Método: tres lanzamientos independientes por caso; cada uno con perfil temporal (`AEROS_CONFIG_DIR`) y proyecto vacío; sin automatización de entrada. Ventana A: CPU entre 1,5 s y 4,5 s después de `WaitForInputIdle`. Ventana B: CPU entre 10 s y 13 s después del lanzamiento, sin ninguna entrada. CPU expresada en porcentaje de un núcleo. La ventana tuvo el foco al inicio y al final de cada muestra (`focus_before`/`focus_after = true`).

| Caso | CPU A (activo) | CPU B (reposo) | Working set | Privada | WaitForInputIdle |
| --- | --- | --- | --- | --- | --- |
| Anterior, Orbital | 18,2–21,4 % | 10,9–19,8 % | 104 MiB | 96 MiB | 836–859 ms |
| Actual, Orbital | 24,5–31,8 % | 25,0–27,1 % | 105 MiB | 93 MiB | 840–849 ms |
| Actual, ASCII Prism | 26,0–29,7 % | 26,0–37,5 % | 106 MiB | 94 MiB | 832–841 ms |
| Actual, fondo Off | 9,4–13,0 % | **0,0 %** | 105 MiB | 92 MiB | 832–834 ms |
| Actual, Off + Reduced Motion | 0,0 % | 0,0–0,5 % | 95 MiB | 103 MiB | 306–329 ms |

Lectura:

- Con el fondo apagado, AEROS no consume CPU en reposo: Luma anima ocho segundos después de cada entrada o cambio de estado y luego queda quieta. En la primera medición de esta sesión (antes de esa corrección) el reposo costaba ~10 % de un núcleo.
- `WaitForInputIdle` volvió a ~0,84 s, igual que la versión anterior. Antes de la corrección marcaba ~1,9 s porque el primer cambio de humor (Sleeping → Idle al recibir foco) abría 1,6 s a 60 fps.
- Con fondos animados el costo es mayor que en la versión anterior: unos 5–10 puntos de un núcleo más, con bastante variación entre corridas (la versión anterior osciló entre 10,9 % y 19,8 % en reposo en la misma sesión). La escena tiene más elementos (tarjetas con sombra, velo, Luma con malla, iconos) y ASCII Flow dibuja hasta ~6.000 celdas por frame a 20 fps. Los fondos siguen pausándose sin foco y con Reduced Motion, y están apagados detrás del código por defecto.
- Memoria estable: +1–2 MiB de working set frente a la versión anterior pese a 116 iconos y marcas embebidas (se decodifican al dibujarse).

`WaitForInputIdle` no es la latencia hasta el primer frame presentado; se usa solo como comparación relativa.

## 2. Construcción de la UI por frame (CPU)

Archivos: `manifest-*.json`, campo `ui_build_cpu_ms`. Mide `Editor::update` (construcción de la UI y comandos de pintura) en cada frame del recorrido de evidencias, con entrada inyectada. No incluye teselado final, GPU ni presentación.

| Recorrido | Frames | p50 | p95 | máx. |
| --- | --- | --- | --- | --- |
| 1366×768 @100 %, completo (temas, fondos, Companion, mascota, idiomas) | 6.815 | 2,21 ms | 4,62 ms | 55,8 ms |
| 1920×1080 @100 % | 921 | 2,30 ms | 2,64 ms | 56,4 ms |
| 1920×1080 @125 % | 905 | 2,26 ms | 2,57 ms | 57,4 ms |
| 1920×1080 @150 % | 953 | 1,25 ms | 1,54 ms | 55,1 ms |
| 1366×768 @125 % | 974 | 1,21 ms | 1,48 ms | 56,3 ms |
| 1366×768 @150 % | 976 | 1,22 ms | 1,53 ms | 56,4 ms |

El máximo (~55 ms) corresponde al primer frame de cada corrida (fuentes, texturas y gramáticas); los percentiles muestran el régimen normal. El recorrido de interacción heredado (`capture_precision_interactions`) registró p50 1,94 ms y p95 3,19 ms para construcción de UI, y entrada→comandos de pintura p50 2,12 ms / p95 7,81 ms (`design/excellence-captures/interactions/interaction-metrics.json`).

## 3. Optimizaciones aplicadas

- Texturas de iconos y marcas: decodificación única por contexto, mipmaps y reutilización por handle.
- Resaltado: `FrameCache` de egui por (texto, lenguaje, tema, tamaño); se recalcula solo al editar.
- Guías y decoraciones del editor: solo filas visibles; búsqueda de corchetes acotada a 40.000 caracteres.
- Luma: reposo sin repintado tras 8 s sin entrada; ráfaga de 60 fps solo para reacciones puntuales; malla reducida.
- Fondos: pausa sin foco, minimizado o con Reduced Motion; apagados detrás del código por defecto.

## No medido

GPU/VRAM, latencia física tecla→píxel, DPI global de Windows distinto de 100 %, proyectos grandes y varias sesiones de agentes autenticadas simultáneas.
