# Estado de requisitos

Clasificación: **Verificado** (implementado, ejecutado en la app real y revisado en capturas o pruebas), **Implementado sin verificar**, **Parcial**, **Pendiente**, **Bloqueado**.

## Recuperación

| Requisito | Estado | Evidencia |
| --- | --- | --- |
| Localizar repositorio, ramas, worktrees y ejecutables | Verificado | 4 worktrees y 6 ramas; historia lineal hasta `7e0d027`. Ver `README.md`. |
| Rama de recuperación y worktree aislado | Verificado | `recovery/visual-excellence` en `E:/workspace/aeros-excellence`, build en `E:/workspace/aeros-excellence-build`. |
| No sobrescribir trabajo ajeno | Verificado | Ninguna otra rama ni worktree se modificó. En `E:/Dev/AEROS/Editor` solo se agregó esta carpeta de evidencias, sin commit. |

## Iconografía

| Requisito | Estado | Evidencia |
| --- | --- | --- |
| Sistema profesional: Material Icon Theme 5.40 (MIT) | Verificado | `02-explorer/`, `assets/icons/material/provenance.json` |
| Rust, Python, Java, JS, TS, C, C++, C#, Go, HTML, CSS, JSON, TOML, YAML, XML, Markdown, Git, Docker, Gradle, Maven, configuración | Verificado por prueba; parcialmente en captura | Prueba `assets::tests::every_mapped_icon_is_bundled_and_decodes_once`. El explorador capturado muestra Rust, Python, TS, Markdown, Git, Docker, JSON, TOML, SVG y solución. |
| Iconos específicos: Cargo.toml, package.json, README, .gitignore, Dockerfile, tsconfig, .sln, LICENSE | Verificado | `02-explorer/explorer-expanded-*.png` |
| Carpetas abiertas y cerradas, carpetas especiales | Verificado | src, tests, docs, assets, target, tools, .github en la captura |
| Mipmaps, ajuste a píxel, nitidez en 125/150 % | Verificado en captura | `*-1366x768@150.png`, `*-1920x1080@125.png` |
| Marcas: Claude, Gemini, Antigravity | Verificado | `04-agent-hub/` |
| Marcas: OpenAI, Codex, ChatGPT | Bloqueado | La descarga oficial exige aceptar los términos de OpenAI. Monograma neutro temporal. Ver `assets/brands/PROVENANCE.md`. |

## Resaltado de sintaxis

| Requisito | Estado | Evidencia |
| --- | --- | --- |
| Auditoría del motor | Verificado | `VISUAL_AUDIT.md` |
| Keywords, declaraciones, tipos, funciones, métodos, macros, propiedades, constantes, strings, escapes, números, operadores, atributos, comentarios, lifetimes, namespaces, errores | Verificado | Pruebas `syntax::tests::*`; capturas Rust, TS, Python y TOML |
| Parámetros e imports | Parcial | Coloreados cuando la gramática los identifica (`variable.parameter`, `entity.name.namespace`). |
| Advertencias de diagnóstico | Pendiente | Requiere LSP. |
| Paletas por tema con contraste ≥ 4,5:1 | Verificado | Prueba `palettes_keep_code_readable`; `07-themes/` |
| Línea activa, guías de indentación, emparejado de corchetes, cursor, número de línea activo | Verificado | `03-editor/*` |
| Ligaduras opcionales | Pendiente | egui no aplica ligaduras OpenType. |
| Gramática TypeScript dedicada | Parcial | Gramática JavaScript más corrección léxica de palabras clave y tipos. |

## Movimiento

| Requisito | Estado | Evidencia |
| --- | --- | --- |
| Sistema compartido (`motion.rs`) y Reduced Motion | Verificado | Prueba `pointer_deforms_each_scene_and_reduced_motion_freezes_it` |
| Pestañas, Command Palette, Companion, tarjetas de Home, filas, chevrons, mensajes, toasts | Verificado | `10-motion/` con tiempos medidos en el manifiesto |
| Springs amortiguados | Parcial | Curvas cubic-out y suavizado exponencial; no hay integrador de resortes. |
| Transición de cambio de tema | Pendiente | El cambio es inmediato. |

## Fondos

| Requisito | Estado | Evidencia |
| --- | --- | --- |
| ASCII Flow con seis estilos de identidad propia | Verificado | `08-ascii-backgrounds/ascii-*` |
| Orbital, Neural, Void y Minimal | Verificado | `08-ascii-backgrounds/` |
| Parallax, repulsión, lente, onda de clic y retorno suave | Verificado | `10-motion/ascii-prism-*` |
| Pausa sin foco y con Reduced Motion | Verificado | Pruebas `identity::*` |
| Apagado detrás del código por defecto | Verificado | Preferencia `behind_code = false` |
| Control de intensidad, velocidad y densidad | Verificado | Settings → Fondo y ASCII |

## Mascota y Companion

| Requisito | Estado | Evidencia |
| --- | --- | --- |
| Mascota original con silueta reconocible, mirada, hover, clic e idle | Verificado | `06-mascot/` |
| Estados Thinking, Coding, Searching, Waiting, Success, Error, Sleeping | Verificado | `06-mascot/`; estados ligados a chats, búsquedas y al puente de agentes |
| Desactivable | Verificado | Settings → Luma |
| Companion compacto y expandido, aprobación, actividad, notificaciones | Verificado | `05-companion/`. La solicitud proviene de un proceso real `--agent-hook` con datos de prueba y se rechaza; ninguna herramienta se ejecuta. |

## Idioma

| Requisito | Estado | Evidencia |
| --- | --- | --- |
| Arquitectura i18n, cambio en vivo, persistencia y fallback | Verificado | `i18n.rs`; pruebas `i18n::tests::*` |
| Español en Settings, Home, editor, Agent Hub y Companion | Verificado | `09-settings-es/` |
| Cobertura del 100 % de las cadenas | Parcial | Algunos mensajes con formato (errores del sistema, conteos de búsqueda, títulos de tareas) siguen en inglés. |
| Contenido de agentes en su idioma original | Verificado | El texto de los mensajes no pasa por la traducción. |

## Funcionalidad

| Requisito | Estado | Evidencia |
| --- | --- | --- |
| Abrir, navegar, guardar, buscar, pestañas y paleta | Verificado | 32 pruebas y el recorrido de interacción |
| Enter para enviar en el chat | Implementado sin verificar con un proveedor autenticado | — |
| Terminal PTY, LSP y Git avanzado | Pendiente | `NEXT_STEPS.md` |
| Resolución 2560×1440 | Bloqueado en este equipo | El monitor es de 1920×1080 y Windows limita la ventana. |
