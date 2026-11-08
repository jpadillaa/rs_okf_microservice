---
type: "Source Section"
unit_id: "7f1ec1565ae0546dbedc5901964d0df31d4314b85e9047384fe787d63e5f9e08/contenido/seccion-003.md"
title: "2. Arquitectura"
description: "Sección 3 de informe_tecnico.pdf: 2. Arquitectura. La nueva plataforma separa la aplicación en tres servicios: catálogo, existencias y reportes. Cada servicio tiene su propia base de datos y se comunica con los…"
sources:
  - id: "original"
    resource: "/references/original.pdf"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "seccion"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 1
title_inferred: true
---

# 2. Arquitectura

> Encabezado inferido por su formato tipográfico: el PDF no lo marca como encabezado.

La nueva plataforma separa la aplicación en tres servicios: catálogo, existencias y reportes. Cada servicio tiene su propia base de datos y se comunica con los demás mediante mensajes.

La separación permite actualizar cada servicio por su cuenta y escalar solo el que recibe más carga. A cambio, exige definir con cuidado los mensajes que intercambian los servicios, porque un cambio en su formato obliga a coordinar las versiones de todos los que los consumen. Para reducir ese riesgo, cada mensaje lleva un número de versión y los servicios aceptan durante un tiempo la versión anterior y la nueva.

La plataforma registra cada operación en un historial que no se puede modificar. El historial sirve para auditar los cambios de existencias, para reconstruir el saldo de cualquier día y para detectar diferencias entre el inventario físico y el registrado en el sistema.

1. El servicio de catálogo guarda la descripción de cada producto, sus códigos y sus proveedores, y publica un mensaje cada vez que cambia un producto.
2. El servicio de existencias registra entradas y salidas por sede y calcula el saldo disponible en tiempo real.
3. El servicio de reportes consolida los datos de los otros dos servicios cada noche.

La configuración mínima de cada servicio se describe con un archivo como el siguiente:

```
servicio: existencias
replicas: 2
base_de_datos:
  motor: postgres
  version: 16
```

