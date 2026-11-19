# Auditoría visual

Criterios verificables por pantalla: jerarquía (un foco claro), contraste de texto (≥ 4,5:1 por prueba automática en temas y paletas de sintaxis), alineación a la retícula de 4 pt, ausencia de solapes y recortes a 100/125/150 %, identidad (violeta desaturado, grafito, geometría de órbita), y movimiento que no bloquea la entrada.

Todas las capturas citadas están en esta carpeta, sufijo `-<ancho>x<alto>@<escala>`.

## Defectos encontrados y corregidos durante esta sesión

| # | Defecto observado en captura | Corrección |
| --- | --- | --- |
| 1 | Resaltado plano: `theme::syntax_color` colapsaba todos los tokens en ~5 colores. | Clasificador semántico propio (`syntax.rs`) con paletas por tema. |
| 2 | Iconos vscode-icons genéricos, 21 tipos, sin carpetas especiales; carpeta pegada al nombre. | Material Icon Theme (116), mipmaps, ajuste a píxel, filas propias con chevron e indentación. |
| 3 | Mascota raster con ojos pintados encima. | Luma vectorial con ocho estados y seguimiento del cursor. |
| 4 | Companion como ventana de texto y botones. | Isla compacta y tarjeta expandida con estados, medidores y aprobación. |
| 5 | Barra de scroll flotante cruzando el arte de Home. | Barra oculta en Home. |
| 6 | Fondo ASCII como ruido uniforme detrás del contenido. | Composición en dos capas, escenario a la derecha, seis estilos. |
| 7 | En Home el contenido centrado tapaba el fondo. | Layout hero en ventanas anchas. |
| 8 | La isla compacta tapaba las pestañas centrales. | Se movió a la barra de estado. |
| 9 | A 150 % el árbol del Explorador se deslizaba bajo Luma. | El dock es un panel inferior real. |
| 10 | TypeScript: `export`, `class`, `const` sin color (gramática JS). | Recuperación léxica de palabras clave y tipos PascalCase. |
| 11 | Guías de indentación ausentes en archivos de 2 espacios. | Unidad de indentación detectada por archivo. |
| 12 | ASCII casi invisible en Light. | Tinta más oscura y ganancia en temas claros. |
| 13 | Fixtures de aprobación escribían en el puente real del usuario. | Arnés aislado con `AEROS_CONFIG_DIR`; se borraron los archivos de prueba que habían quedado. |

## Evaluación por pantalla

| Pantalla | Evidencia | Resultado | Observaciones |
| --- | --- | --- | --- |
| Home | `01-home/` | Cumple | Hero con tarjetas; hover eleva la tarjeta y enciende el borde. A 150 % pasa a una columna. |
| Explorador | `02-explorer/` | Cumple | Iconos con colores originales, chevrons, guías y selección con marca de acento. |
| Editor Rust | `03-editor/rust/` | Cumple | Declaraciones violeta, tipos verde agua, funciones azul, macros naranja, literales cálidos, comentarios en cursiva atenuada; línea activa, guías y corchetes. |
| Editor TypeScript | `03-editor/typescript/` | Cumple con nota | Gramática JS con corrección léxica; template strings y genéricos legibles. |
| Editor Python | `03-editor/python/` | Cumple | Decoradores, f-strings, docstrings y anotaciones diferenciadas. |
| TOML | `03-editor/rust/editor-toml-*` | Cumple | Tablas, claves, strings, números y booleanos. |
| Command Palette | `03-editor/command-palette-*` | Cumple | Sin cambios de layout en esta fase. |
| Agent Hub | `04-agent-hub/` | Cumple | Identidad del proveedor, burbujas, bloque de código resaltado, tarjetas de agentes con estado. |
| Companion | `05-companion/` | Cumple | Isla en la barra de estado con medidor; tarjeta con aprobación destacada en ámbar. |
| Mascota | `06-mascot/` | Cumple | Recortes ampliados ×5 de la captura real. Ocho estados distinguibles por forma y color, no solo por color. |
| Temas | `07-themes/` | Cumple | Dark, Midnight, Violet, Light y High Contrast; sintaxis con paleta propia en Light y High Contrast. |
| Fondos | `08-ascii-backgrounds/` | Cumple con nota | Prism y Orbit tienen la composición más fuerte; Contour y Signal son deliberadamente sobrios. |
| Español | `09-settings-es/` | Cumple con nota | Settings, Home, editor, Agent Hub y Companion en español; quedan mensajes con formato en inglés. |
| Movimiento | `10-motion/` | Cumple | Secuencias reales de pestaña, paleta, isla, lente y onda; el manifiesto registra milisegundos desde la acción. |

## Límites de esta auditoría

- La entrada se inyecta en la ventana nativa con `raw_input_hook` de eframe. El control del escritorio fue denegado; no se usó SendInput ni ratón físico.
- El monitor es de 1920×1080: la petición de 2560×1440 queda limitada por Windows y no se publica.
- Las escalas 125/150 % se aplican con el `pixels_per_point` de egui dentro de una ventana real del tamaño físico indicado; no cambian el DPI global de Windows.
- No hay lector de pantalla ni prueba de daltonismo automatizada.
