# Spec original

Tal como se pidió. "Yo" es el agente (Claude). Lo que cambió al construirlo está en
[PLAN.md](PLAN.md), sección "Decisiones".

## Idea central

La app es la única que tiene las credenciales y la que abre las conexiones SSH. El agente nunca
recibe una contraseña o una clave: le pide a la app "corré este comando en el host X" y recibe solo
la salida. La persona maneja todo desde una interfaz gráfica parecida a Termius, y también la usa
como cliente SSH.

## 1. Componentes

- Servicio de fondo (broker): guarda el vault, abre las conexiones SSH, aplica las reglas de qué se
  puede correr y registra todo. Puede correr en la PC o en un servidor, para usarlo desde varias PCs.
- Interfaz web o de escritorio (tipo Termius): hosts, grupos, credenciales, reglas, aprobaciones y
  auditoría. Terminal interactiva.
- Conexión para agentes (MCP): local en la PC, o por HTTPS con un token por PC si el broker está en
  un servidor.

## 2. Vault

- Tipos de secreto: contraseñas, claves privadas (con o sin passphrase), certificados SSH,
  contraseña de sudo, tokens de API.
- Cifrado de todo el vault (AES-256-GCM) con una clave derivada de una contraseña maestra
  (Argon2id), o protegida por el sistema operativo (DPAPI en Windows, Keychain en Mac).
- Solo una persona puede desbloquearlo, desde la interfaz. Se bloquea solo después de un tiempo sin
  uso. El agente no tiene ninguna forma de desbloquearlo.
- Los hosts referencian credenciales por ID (`cred: bastion-key`). El agente solo conoce ese ID.
- Importar desde `~/.ssh/config`, desde Termius y desde archivos de clave.
- Exportar fácil: Bitwarden, Termius y otros formatos comunes (agregado después).

## 3. Hosts

- Nombre, IP o hostname, puerto, usuario, credencial, grupo, etiquetas y entorno (prod, test, dev).
- Saltos por bastión (ProxyJump encadenado).
- Verificación de la clave del host: la primera vez muestra la huella y la tiene que confirmar una
  persona. Si después cambia, se bloquea la conexión y se avisa.
- Una descripción del host que el agente puede leer para tener contexto.

## 4. Lo que puede hacer el agente

- Listar hosts y grupos: nombre, entorno, etiquetas y descripción; nunca IPs, credenciales ni secretos.
- `run(host, comando)`: salida, código de salida y duración.
- Opcionales por host: sesiones persistentes, leer archivos, subir y bajar archivos, túneles.
- sudo sin exponer la contraseña: la app la inyecta por la entrada estándar. Nunca en la línea de
  comando, ni en variables de entorno, ni en la salida.

## 5. Reglas por host o por grupo

- Modos: solo lectura (lista de permitidos), con aprobación, libre.
- Lista de prohibidos siempre: `rm -rf`, `shutdown`, `mkfs`, ver el historial de la shell, leer
  `~/.ssh` y `/etc/shadow`.
- Los comandos se analizan completos, incluidos pipes, `;`, `$( )` y redirecciones.
- Límites de tiempo, de tamaño de salida y de comandos por minuto.

## 6. Aprobaciones

- Cola con el comando, el host y el motivo que da el agente.
- Aprobar, rechazar, aprobar este patrón por la sesión, o editar el comando antes de aprobarlo.
- Notificación en el escritorio o en el celular.

## 7. Protección de la salida

- Se buscan todos los secretos del vault en la salida y se reemplazan por `***`.
- También patrones conocidos: claves privadas, tokens, JWT, `password=`, cabeceras Authorization y
  cadenas de conexión con contraseña.

## 8. Auditoría

- Cada comando: quién, desde qué PC, host, comando, salida (o su hash), resultado y quién lo aprobó.
- Grabación opcional de sesiones; búsqueda y exportación desde la interfaz.

## 9. Varias PCs

- Modo servidor: las PCs se conectan con un token de dispositivo con vencimiento, hosts permitidos y
  revocación inmediata.
- Botón de apagado general que corta todo el acceso del agente.

## 10. Fuera de alcance

Que el agente desbloquee el vault, exporte o lea secretos, o cambie las reglas o las credenciales.

## 11. Pruebas de aceptación

1. El agente pide un `cat` de un archivo que contiene una contraseña del vault y recibe `***`.
2. sudo funciona sin que la contraseña aparezca en la salida, la auditoría ni la lista de procesos.
3. Un comando prohibido escondido en un pipe o en un `$( )` se rechaza.
4. Con el vault bloqueado, todo pedido falla.
5. Si cambia la clave de un host, la conexión se bloquea.

## Etapas pedidas

1. Vault con hosts, la conexión para agentes con run, enmascarado y auditoría.
2. Reglas y aprobaciones.
3. Modo servidor para varias PCs.
4. Interfaz completa con terminal interactiva.

Estilo: moderno, barato de modificar, liviano y que corra en segundo plano sin drama. Temas extra
inspirados en Warp, Poolsuite FM y Haze.
