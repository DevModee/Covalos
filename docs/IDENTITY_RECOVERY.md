# AEROS identity recovery

## Referencias comprobadas

Coucou: https://github.com/louis-cfm/coucou — código MIT; nombre, personaje, sonidos y recursos visuales tienen condiciones separadas en LICENSE-ASSETS.md. AEROS usa una implementación propia y Luma, una sonda orbital con forma de cometa, dibujada con primitivas vectoriales originales. No se reutilizan sus recursos.

Contrato de eventos consultado: https://github.com/louis-cfm/coucou/blob/main/docs/AGENTS.md. El puente AEROS recibe nombres canónicos de hooks y conserva sesión, operación y alcance. Las decisiones para Claude siguen https://code.claude.com/docs/en/hooks, sección PermissionRequest.

Obscura: https://github.com/h4ckf0r0day/obscura, Apache-2.0, navegador headless para automatización. Su integración de navegador es independiente de la identidad y queda pendiente; no se incorpora un segundo motor ni se descargan binarios.

Las cinco publicaciones de X incluidas en la directiva devolvieron acceso no disponible: AlbiaHossain/2122644960118026264, Skirook/2122768045982773575, AdelineAstraOS/2122328210989883731, sarthakguptadev/2122214682924900813 y noechague/2122766731500490994. No se atribuyen formas o animaciones a contenido no visto.

La revisión de las cinco skills de diseño y sus licencias está en DESIGN.md. Se aplican sus principios transferibles a egui; no se ejecutaron sus scripts.

## Implementado

Cinco temas semánticos completos y persistentes. Fondos Orbital, Neural, Void, ASCII, Minimal y Off. ASCII con seis patrones; controles de FPS, velocidad, densidad, intensidad y color. El área de código queda sin animación por defecto y requiere activación explícita. Se respeta movimiento reducido, foco y minimización. Los relojes de fondo y mascota son independientes para que una tarea activa no mueva un fondo estático.

Luma usa estados de solicitudes reales, búsquedas y propuestas pendientes. No crea agentes ficticios. Puede ocultarse, cambiar de tamaño y desactivar movimiento o interacción. El dibujado usa coordenadas lógicas para HiDPI.

## Verificación y límites

Las capturas están en design/quality-captures. Se generan desde el framebuffer de ventanas eframe reales; los datos de conversación de los tests se identifican como fixtures reproducibles, no respuestas en vivo. La escala 125/150 % usa egui en una ventana nativa; no equivale a cambiar globalmente el DPI de Windows.

Falta completar la comprobación manual entre monitores, lector de pantalla y Snap. La prueba de rendimiento final y la guía de integración se registran al cerrar el checkpoint.
