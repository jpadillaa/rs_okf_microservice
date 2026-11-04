# PostgreSQL como único motor

**Fecha:** 3 de noviembre de 2026. **Decisión:** sustituir SQLite por PostgreSQL a petición del usuario, con conexión configurable desde un archivo de variables y una instancia Docker de prueba.

## Persistencia y concurrencia

SQLx 0.9 usa conexiones asíncronas (`PgPool`, máximo cinco por proceso, cinco segundos para adquirir una conexión). `jobs`, `worker_heartbeats` y `conversion_metrics` mantienen sus responsabilidades. Fechas Unix, intentos y acumulados se guardan como `BIGINT`; las rutas y estados siguen siendo texto. `_sqlx_migrations` registra las migraciones embebidas de `migrations/`, aplicadas al arrancar con el bloqueo de migración de SQLx. El servicio no abre archivos SQLite y no hay selector de motor.

La reclamación bloquea una fila con `FOR UPDATE SKIP LOCKED`, cambia el estado y confirma antes de convertir. Las asignaciones siguen venciendo y renovándose. La recuperación toma filas vencidas con el mismo mecanismo. Publicación, fallo, apagado y reconciliación bloquean la fila mientras coordinan archivos y estado; un intento que perdió la asignación no borra archivos del intento vigente. La retención publica primero `expired` y después bloquea las filas pendientes de limpieza para repartirlas entre workers. Los contadores se acumulan mediante `ON CONFLICT`, usando `GREATEST` para la memoria máxima.

La base no almacena documentos ni reemplaza el volumen de archivos. API y workers deben compartir el mismo almacén. El despliegue multinodo sigue fuera del alcance: requiere resolver también la publicación y disponibilidad de los archivos.

## Configuración y operación

El [README](../../README.md) explica `.env`, `--env-file`, la prioridad del entorno del proceso y las URLs para host, Compose y pruebas. `.env.example` tiene valores genéricos y `.env` no se versiona. Una URL ausente, un archivo explícito inexistente o una configuración inválida detienen el arranque; no hay retorno a SQLite. Para bases externas se puede configurar TLS en la URL, incluido `sslmode=verify-full` y `sslrootcert`.

Compose levanta PostgreSQL 17 con imagen fijada por digest, volumen propio, comprobación de salud y puerto publicado solo en localhost. Las migraciones se ejecutan en la API y en el worker y requieren permisos sobre el esquema de la aplicación. La cuenta genérica de desarrollo sirve para pruebas; una instalación propia debe usar sus credenciales y política de copias de respaldo.

El supervisor necesita red hacia PostgreSQL. El hijo recibe solo el entorno de conversión permitido, sin credenciales ni carga de `.env`. En Linux, seccomp rechaza la creación de sockets, conexiones, `ptrace` e `io_uring_setup`; comprueba la arquitectura y rechaza la ABI x32. Se mantiene Landlock para rutas y los límites de recursos. Si seccomp no se instala, la conversión falla. En macOS esa capa de red no se aplica; la garantía se verifica en Linux/Docker.

## Importar un registro SQLite anterior

La importación es una herramienta de mantenimiento independiente, no un segundo motor del servicio:

1. Detener API y workers antiguos y conservar una copia de SQLite y de su almacén completo. No debe haber trabajos `processing`.
2. Arrancar una vez la API nueva para aplicar el esquema PostgreSQL y detenerla. El destino debe estar vacío.
3. Preparar `.env` con la conexión de destino y mantener los archivos en las mismas rutas relativas. Si el destino es Compose, copiar el almacén al volumen `okf-data` antes de iniciar los consumidores.
4. Comprobar y después importar:

```bash
uv run tools/migrate_sqlite.py data/jobs.sqlite3 --env-file .env --data-dir data
uv run tools/migrate_sqlite.py data/jobs.sqlite3 --env-file .env --data-dir data --apply
```

Sin `--apply`, todas las inserciones se revierten. La importación bloquea las tablas destino, rechaza un destino ocupado, comprueba las rutas y los archivos referenciados y copia trabajos y métricas en una transacción. No importa latidos antiguos. Los trabajos terminados de versiones sin `finished_at` empiezan su retención al importar. La herramienta abre SQLite en solo lectura y no copia, borra ni publica archivos. Conserva la base anterior hasta verificar el nuevo servicio.

## Verificación

Las pruebas de servicio, salud, retención, apagado y procesos usan PostgreSQL real. Cada raíz temporal obtiene un esquema independiente en la base indicada por `OKF_TEST_DATABASE_URL`; no se permite usar implícitamente la conexión del servicio. Se comprueba la prioridad de configuración y la reclamación concurrente, incluida una fila bloqueada y una asignación vencida. La prueba de humo crea su propio PostgreSQL temporal y convierte y valida una muestra de cada formato. Los golden y el perfil documental no cambian: esta decisión modifica la persistencia y la operación, no las reglas del bundle.
