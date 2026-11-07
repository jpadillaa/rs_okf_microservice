# Diagrama conceptual del microservicio de conversión a OKF

```mermaid
flowchart TD
    A["Recepción y registro"] --> B["Inspección de la fuente"]
    B --> C["Extractor por formato"]
    C -. "PDF escaneado, con OKF_OCR_ENABLED" .-> O["OCR local"]
    O -.-> D
    C --> D["Modelo de lectura del formato"]
    D --> E["Reconciliación editorial"]
    E --> F["Ensamblado OKF desde el modelo de salida común"]
    F --> G["Conformidad OKF"]
    F --> H["Integridad editorial"]
    G --> I{"Estado final"}
    H --> I
    I --> J["Bundle e informe"]
```

La recepción registra el trabajo y entrega su identificador. Un worker ejecuta las etapas posteriores de forma asíncrona. Las dos validaciones son independientes y determinan el estado del resultado. El OCR local es opcional: solo actúa sobre las páginas PDF escaneadas sin capa de texto, y su texto entra en el mismo modelo de lectura que el texto digital ([OCR local](../decisiones/OCR_LOCAL.md)).
