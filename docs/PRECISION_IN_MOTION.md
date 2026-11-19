# AEROS — un único sistema visual

Rama: design/precision-in-motion. Worktree: E:/workspace/aeros-precision-motion.
Base íntegra: 17a2998, sobre improvement/autonomous-quality. Build aislado: E:/workspace/aeros-precision-build.

Los prompts Visual Perfection y Project Apex se ejecutan como una sola evolución de la base existente. No se crearon dos diseños alternativos. Se conservaron el framework nativo eframe/egui, los cinco temas, los seis presets y patrones existentes, los proveedores, Companion, hooks y aprobaciones, archivos, terminal, tareas y recuperación de conversaciones. Esta fase no añade un orquestador ni adaptadores nuevos.

## Dirección y contratos

Geist e IBM Plex Mono permanecen como fuentes. Grafito y violeta desaturado; superficies sobrias, código limpio y decoración en Home, Hub y estados vacíos. El chat usa una superficie opaca en Hub para proteger la lectura. Los colores siguen saliendo de las mismas paletas semánticas.

El movimiento se centraliza en src/editor/motion.rs: hover 120 ms, controles 160 ms, paneles 240 ms y entradas de vista 280 ms; Command Palette usa 200 ms. La entrada no espera a la animación. Reduced Motion y pérdida de foco anulan las transiciones decorativas. El panel derecho mantiene su contenido y foco durante la apertura/cierre. El indicador activo se desplaza entre pestañas; las filas interpolan hover y selección; el composer tiene foco animado; los mensajes y notificaciones tienen entrada breve, sin escritura artificial de respuestas recibidas.

Los fondos observan el puntero y los clics sin capturar eventos. Hay parallax, repulsión local, órbitas con iluminación y esfera, red de proximidad, campo ASCII continuo y ondas de interacción. La geometría fija de las órbitas se tessella y cachea; partículas y puntero se actualizan aparte. La reactividad temporal usa tiempo real y pausa sin foco/minimizada. El render de código conserva los fondos apagados por defecto. La interacción puede pedir 60 repintados/s temporalmente; se usa la cadencia ambiental configurada al quedar en reposo.

Luma es una polilla celeste original con textura transparente de 256 px, mirada y expresiones nativas. Vive en un espacio inferior propio, sin ocupar filas del explorador. Tiene reacción al hover y clic, respiración breve, descanso, cambios de mirada para tareas reales y estados de aprobación, error y éxito. Se puede ocultar el dock, desactivar el movimiento, regular tamaño e intensidad. Las ilustraciones de identidad de los paneles son estáticas. No se copiaron Mochi, sonidos ni arte de Coucou.

## Inventario integrado

| Requisito | Resultado verificable |
| --- | --- |
| Iconos de archivos | 21 SVG upstream y sus PNG de 64 px; iconos coherentes en árbol y pestañas, carpetas abiertas/cerradas y nombres especiales. Decodificación RGBA y texturas cacheadas. |
| Explorador | Guías, indentación, hover/selección, tooltips, carpetas animadas y archivos modificados conservados. |
| Fondos | Orbital, Neural, ASCII y Void interactivos; Off/Minimal conservados; seis patrones ASCII y controles persistentes. |
| Mascota | Arte original integrado, movimiento contextual, mirada, dock propio y ajustes. |
| Chat/Hub | Encabezado tipográfico, estado vacío diseñado, composer con foco, entrada de mensajes, streaming y código existentes; superficie de lectura protegida. |
| Companion | Cabecera gráfica, sesiones jerarquizadas, estados reales y decisiones existentes; tamaño adaptado cuando no hay sesiones; registros secundarios. |
| Temas | Dark, Midnight, Violet, Light y High Contrast; vistas previas y persistencia; capturas de 15 superficies por tema. Light se verificó con escala egui de 150 %. |
| Conservación funcional | Pruebas de archivo/guardado/búsqueda/terminal/paleta, conversaciones recuperables y contratos de aprobación. |
| Marcas de proveedores | Registro central de nombres, propietarios y fuentes. Se retiró el símbolo genérico del encabezado del proveedor. Logos de producto pendientes de autorización/revisión; no se sustituyen por identidades inventadas. |

## Evidencia y correcciones

1. Primer pase ejecutado y capturado en design/precision-captures/pass-1. Se detectaron entrada de Home demasiado tenue en capturas tempranas, fondo que competía con el chat y un Companion vacío demasiado alto.
2. Se corrigieron superficie de lectura, proporciones, estado vacío y espera de captura. El recorrido nativo reveló ASCII blanco sobre Light: se cambió el override de color del galley. La animación del panel derecho mostró una reducción de ancho no deseada: se corrigió preservando su extensión y el ajuste manual.
3. Se ejecutaron clics y teclado reales para egui mediante el raw_input_hook de eframe, dentro de una ventana nativa, dirigidos a los rectángulos devueltos por los controles ya renderizados. Se abrió src/main.rs, escribió y guardó un archivo temporal, buscó texto, cambió pestañas, abrió/cerró la paleta, abrió Companion y seleccionó tema y fondos desde Settings. El control de escritorio externo falló al iniciar; no se presenta esta prueba como validación de SendInput o del escritorio completo.
4. Las capturas de aprobación emplean un subprocess real --agent-hook y un payload explícito de prueba. El hook se rechaza después. No se ejecuta la operación ni se representa como una sesión autenticada de un modelo.

Capturas actuales: [Home](../design/precision-captures/final/dark/home.png), [editor](../design/precision-captures/final/dark/editor.png), [Agent Hub](../design/precision-captures/final/dark/agent-hub.png), [Companion y aprobación de prueba](../design/precision-captures/final/dark/companion.png), [Light 150 %](../design/precision-captures/final/light/settings.png), [pestaña en movimiento](../design/precision-captures/final/release-interactions/tab-moving.png), [pestaña asentada](../design/precision-captures/final/release-interactions/tab-settled.png), [ASCII Light](../design/precision-captures/final/release-interactions/ascii-right.png).

## Rendimiento y alcance de las mediciones

La prueba nativa Release registra construcción de UI y comandos de pintura en CPU, con instrumentación de objetivos. Sus resultados están en design/precision-captures/final/release-interactions/interaction-metrics.json. El tiempo no incluye tessellation final, presentación GPU ni latencia física de teclado/pantalla. No equivale a una garantía de FPS o input lag.

Se conservan las muestras comparativas de proceso en design/precision-performance-first-pass.json y precision-performance.json: tres lanzamientos por caso, perfiles y proyectos temporales, 1,5 s de calentamiento y 1 s de medición de CPU. La CPU es porcentaje de un núcleo. Solo son comparables muestras con el mismo foco al inicio y final. El primer pase detectó un mayor costo de Orbital; motivó cachear sus órbitas y sustituir el decodificador genérico por PNG directo. En la repetición posterior no hubo muestras activas coincidentes suficientes: no se afirma una mejora global de CPU ni arranque. Las muestras sin foco y sin efectos registraron 0 % durante su ventana. La memoria del proceso con la nueva escena ronda 94–109 MiB en la repetición; ver cada muestra y su foco antes de comparar.

WaitForInputIdle no mide el primer frame presentado y puede incluir las transiciones de entrada. No se atribuye a ese dato una regresión o mejora de latencia de arranque. GPU/VRAM, latencia física, monitores con DPI global diferente, grandes proyectos y varias sesiones autenticadas simultáneas siguen sin medición en esta fase.

## Referencias y recursos

- [vscode-icons](https://github.com/vscode-icons/vscode-icons): subconjunto MIT, licencia completa y procedencia en assets/icons/vscode/. Se evaluó también [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme); se eligió un solo conjunto por sus variantes abiertas/cerradas y cobertura pequeña coherente.
- [Zed](https://zed.dev/): se inspeccionó visualmente su captura oficial de producto para densidad y separación del área de código.
- [Coucou](https://github.com/Louis-CFM/coucou): se revisaron design/prototype/notch-buddy.html, capturas de overview/aprobación y LICENSE-ASSETS.md. Solo se toman decisiones de jerarquía y supervisión; no se distribuye su contenido protegido.
- [OpenAI](https://openai.com/brand/), [Anthropic](https://www.anthropic.com/legal), [Antigravity Press](https://antigravity.google/press) y [Google Brand Resource Center](https://about.google/brand-resource-center/guidance/): fuentes del registro y revisión de marcas. Los permisos pendientes usan nombres neutrales.
- Los cuatro posts de X y la referencia de mascota devolvieron error de acceso. Se buscaron autores y proyectos alternativos; se localizó material relacionado de Skirook, pero no se verificaron los videos exactos. Los fondos de esta rama son originales; no se afirma reproducción fiel de esos videos.

## Checkpoint y criterios todavía abiertos

No se considera cerrada toda la fase visual. Quedan la incorporación de logos de producto con permiso confirmado, observación de los videos exactos de X, revisión con entrada física mediante el controlador de escritorio y mediciones completas de GPU/latencia/DPI. El cierre de pestañas y la apertura del terminal conservan respuesta inmediata; no tienen todavía una transición espacial propia. Los cambios se mantienen aislados: no se fusionan ramas activas ni se reemplaza el ejecutable que otros agentes estaban usando.

Para continuar: usar esta rama, conservar los mismos tokens/paletas/registro y repetir los recorridos nativos; no crear otro sistema visual paralelo. Ejecutable: E:/workspace/aeros-precision-build/release/aeros-editor.exe.

Validación final: 26 regresiones aprobadas, Clippy estricto y formato aprobados; recorrido nativo Release y 75 capturas de superficies en cinco temas. El conjunto final añade 13 capturas de interacciones: 88 PNG. La captura de aprobación ahora espera el estado Waiting recibido por la UI y 350 ms de asentamiento; no usa un contador de frames como sustituto del evento.

La última medición instrumentada Release registró 3.619 muestras de construcción de UI: mediana 0,414 ms y p95 1,202 ms; entrada de prueba hasta esos comandos de pintura, mediana 0,689 ms y p95 6,867 ms. Son muestras del recorrido descrito, con sus límites; no una medida de latencia física.
