# AEROS — Design System Rebuild

Rama `design/system-rebuild` (worktree `E:/workspace/aeros-excellence`), sobre `recovery/visual-excellence`. Renderer: eframe/egui 0.33.3 con backend OpenGL (glow). Todo lo que sigue está implementado en Rust dentro de ese renderer; no se agregó ningún runtime web.

## 1. Referencias verificadas

| Referencia | Cómo se verificó | Qué se tomó |
| --- | --- | --- |
| x.com/sarthakguptadev/status/2122214682924900813 (fondo principal) | Video original (1802×1080, 10,9 s) reproducido y revisado cuadro por cuadro en el navegador integrado: 0,5 s, 3 s, 5,2 s, 6 s, 6,5 s, 7,5 s, 9,5 s | Campo de píxeles cuadrados en grilla fija con separación visible; nubes grises que derivan, cuantizadas por tramado ordenado; el cursor pinta píxeles en violeta que se apagan; el clic deja una mancha violeta y un anillo tramado que se expande y se disuelve; contenido nítido encima; fondo casi negro neutro |
| React Bits — Pixel Blast | Demo en reactbits.dev (preview + código). Su descripción coincide con el video: "Dithered pixel clouds that drift, shimmer and light up under the cursor, with ripples" | Confirmó la técnica: ruido de valor fbm → tramado Bayer 8×8 → píxeles cuadrados; estela del puntero con decaimiento; ondas por clic |
| x.com/praveenisomer/status/2123061164276101240 ("Dither + ASCII + Motion") | Video original (1920×1418, 11,4 s) en 0,9 s, 4,5 s, 8,5 s | Una forma grande y suave en movimiento lento, tramada con matriz ordenada en un solo tinte lavanda. Se adaptó como preset "Dither Bloom" |
| x.com/seanaiux/status/2123045823877644613 (dashboards) | Video original (1920×1080, 13,4 s) en 2 s, 7 s, 11,5 s | Superficies casi negras separadas por valor, bordes de 1 px, etiquetas pequeñas en mayúsculas, iconos de línea de 16 px, filas seleccionadas con fondo redondeado de 6 px, guías punteadas en jerarquías, chips de atajos. No se convirtió AEROS en dashboard |
| React Bits (reactbits.dev) | Catálogo completo de Backgrounds/Animations/Components/Micro revisado; Pixel Blast inspeccionado en vivo | Licencia MIT + Commons Clause. No se copió código: el shader de AEROS es una implementación propia de la técnica |
| useAnimations (github.com/useAnimations/react-useanimations) | README y catálogo | 300 iconos animados en Lottie, MIT. AEROS no tiene reproductor Lottie; se adoptó el patrón (transición de estado del icono: chevron que rota, hover que tiñe) sobre los iconos Lucide |
| ApexUI (apex-ui.in) | Sitio en vivo | React + Tailwind + GSAP + Framer Motion, MIT. Patrones tomados: superficies con borde fino, fondo de grilla tenue, transiciones de hover cortas |
| Sora UI (ui.soralabs.studio) | Documentación | Motion + GSAP + Base UI/Radix. Patrón tomado: springs de Motion como motor de microinteracciones, interrumpibles |
| Tweenly (github.com/mirukullacode/Tweenly) | README | "Motion layer for shadcn/ui", MIT, basado en Motion y GSAP; no publica constantes propias de springs |
| DesignRefs (designrefs.com) | Sitio | Directorio de 441 referencias (UI/UX, tipografía, iconos, color). Usado como índice; las decisiones concretas están en esta tabla |
| IntelliJ IDEA (nueva UI) | Conocimiento del producto | Paneles como "islas" redondeadas sobre un lienzo más oscuro, pestañas compactas, iconos de 16 px de trazo fino |
| Editor minimalista del usuario | **No disponible en esta sesión.** Las imágenes en `target/v3-references` son capturas antiguas de AEROS | La paleta se calibró con la especificación escrita del encargo (fondo negro violáceo, keywords rosa/lila, strings aguamarina, funciones amarillo cálido, tipos azul/violeta claro, constantes naranja suave, comentarios gris desaturado, selección violeta translúcida, cursor rosa) |

## 2. Bibliotecas

| Biblioteca | Licencia | Integración |
| --- | --- | --- |
| Lucide 0.460.0 (`lucide-static`) | ISC | **Integrada.** 71 iconos de interfaz rasterizados a máscaras blancas de 64 px con trazo 1,75 (`assets/icons/ui`, tabla generada `src/editor/ui_icons.rs`), con mipmaps y teñidos al dibujar. Familia única para toda la interfaz |
| Material Icon Theme 5.40.0 | MIT | **Integrada** (fase anterior) para archivos y carpetas |
| OpenAI Blossom (paquete oficial `openai-logos.zip`) | Términos de marca de OpenAI, aceptados por el propietario el 2026-11-19 | **Integrado** para Codex, monocromo en el color del texto |
| Simple Icons (Claude, Gemini) / Antigravity local | CC0 / marca oficial | Integradas en la fase anterior |
| React Bits, ApexUI, Sora UI, Tweenly, useAnimations | MIT (+ Commons Clause en React Bits) | **Adaptadas, no integradas**: dependen de React, Tailwind, Motion, GSAP o Lottie. Se tradujeron sus patrones al renderer nativo |
| Phosphor, Tabler, Material Symbols | MIT / MIT / Apache-2.0 | Evaluadas por documentación (no en demo). Se eligió Lucide por su grilla de 24 px y trazo uniforme, que a 16 px se lee como los iconos de IntelliJ; mezclar familias rompería el peso visual |
| vscode-icons | MIT (iconos CC BY-SA) | Evaluada por documentación. Se mantuvo Material Icon Theme por coherencia con la fase anterior y licencia MIT simple |

## 3. Adaptaciones al renderer nativo

- **Dither Field** (`src/editor/dither.rs`): shader GLSL propio ejecutado mediante `egui::PaintCallback` sobre glow. Ruido de valor 3D en 4 octavas, umbral Bayer 8×8 recursivo, píxeles de 4 pt con separación, estela de 16 muestras del puntero (0,9 s), hasta 6 ondas por clic (2,4 s), viñeta en los bordes. La CPU solo envía uniformes. Si el contexto no admite GLSL 1.40/ES 3.0, cae al fondo Minimal.
- **Motion** (`src/editor/motion.rs`): integrador de resorte amortiguado (modelo rigidez/amortiguamiento de Motion) con tres presets cercanos al amortiguamiento crítico: SNAPPY (hover, ~120 ms), SMOOTH (pestañas, panel lateral, ~220 ms), GENTLE (isla del Companion, ~320 ms). Interrumpible, con intensidad global (0 = instantáneo) y Reduced Motion.
- **Iconos animados** (patrón de useAnimations): chevron del Explorador que rota con el estado, hover que tiñe el icono con un resorte.
- **Superficies** (patrón IntelliJ/ApexUI/dashboards): paneles como islas de radio 10 separadas 6 pt sobre el lienzo, borde de 1 px.
- **Clean Code** (`src/editor/clean.rs`): ventanas egui sin barra de título, radio 14, sombra suave, sobre el mismo `Document`. Desprender, mover, redimensionar, varias a la vez, modo enfoque y acople con transición hacia la barra de pestañas.

## 4. Paleta Nocturne

Interfaz: lienzo `#09080D`, superficie `#110F17`, código `#0D0B13`, elevado `#1A1723`, borde `#231F2E`, texto `#E8E4F0`, acento lila `#C9A4FF`, cursor `#FF7AC6`, selección `rgba(150,104,255,0.28)`.

Código: control de flujo `#F392D3`, declaraciones `#C9A2FF`, tipos `#92B6FF`, funciones `#F6D58E`, métodos `#EBCB8B`, macros `#8FD6F2`, strings `#86E1C1`, números y constantes `#F8A97E`, comentarios `#6F6A83`, puntuación `#8C85A1`. Todos los tokens de lectura superan 4,5:1 sobre el fondo del código (prueba `palettes_keep_code_readable`). La paleta anterior sigue disponible como "Graphite".
