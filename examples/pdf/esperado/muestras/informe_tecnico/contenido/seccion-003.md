---
type: "Source Section"
unit_id: "2670da573f38b94708b33c6720fc15e23efedf95e1fbaff90dc6270364c47646/contenido/seccion-003.md"
parent: "documento.md"
previous: "contenido/seccion-002.md"
next: "contenido/seccion-004.md"
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
  - id: "u-8"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 1
    boxes:
      - page: 1
        x0: 72.00
        top: 310.83
        x1: 525.90
        bottom: 334.83
  - id: "u-9"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 1
    boxes:
      - page: 1
        x0: 72.00
        top: 343.83
        x1: 539.31
        bottom: 408.33
  - id: "u-10"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 1
    boxes:
      - page: 1
        x0: 72.00
        top: 417.33
        x1: 527.72
        bottom: 454.83
  - id: "u-11-13"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 1
    boxes:
      - page: 1
        x0: 78.00
        top: 463.83
        x1: 530.18
        bottom: 534.33
  - id: "u-14"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 1
    boxes:
      - page: 1
        x0: 72.00
        top: 546.33
        x1: 483.30
        bottom: 556.83
  - id: "u-15"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 1
    boxes:
      - page: 1
        x0: 84.00
        top: 565.75
        x1: 197.40
        bottom: 620.75
title_inferred: true
---

# 2. Arquitectura

<!-- okf-generated -->
> Encabezado inferido por su formato tipográfico: el PDF no lo marca como encabezado.

<!-- okf-source-span: u-8 -->
La nueva plataforma separa la aplicación en tres servicios: catálogo, existencias y reportes. Cada servicio tiene su propia base de datos y se comunica con los demás mediante mensajes.

<!-- okf-source-span: u-9 -->
La separación permite actualizar cada servicio por su cuenta y escalar solo el que recibe más carga. A cambio, exige definir con cuidado los mensajes que intercambian los servicios, porque un cambio en su formato obliga a coordinar las versiones de todos los que los consumen. Para reducir ese riesgo, cada mensaje lleva un número de versión y los servicios aceptan durante un tiempo la versión anterior y la nueva.

<!-- okf-source-span: u-10 -->
La plataforma registra cada operación en un historial que no se puede modificar. El historial sirve para auditar los cambios de existencias, para reconstruir el saldo de cualquier día y para detectar diferencias entre el inventario físico y el registrado en el sistema.

<!-- okf-source-span: u-11-13 -->
1. El servicio de catálogo guarda la descripción de cada producto, sus códigos y sus proveedores, y publica un mensaje cada vez que cambia un producto.
2. El servicio de existencias registra entradas y salidas por sede y calcula el saldo disponible en tiempo real.
3. El servicio de reportes consolida los datos de los otros dos servicios cada noche.

<!-- okf-source-span: u-14 -->
La configuración mínima de cada servicio se describe con un archivo como el siguiente:

<!-- okf-source-span: u-15 -->
```
servicio: existencias
replicas: 2
base_de_datos:
  motor: postgres
  version: 16
```

