# Diagrama conceptual del microservicio de conversión a OKF

```mermaid
flowchart TD
    A["Recepción y registro"] --> B["Inspección de la fuente"]
    B --> C["Extractor por formato"]
    C --> D["Modelo documental intermedio"]
    D --> E["Reconciliación editorial"]
    E --> F["Ensamblado OKF"]
    F --> G["Conformidad OKF"]
    F --> H["Integridad editorial"]
    G --> I{"Estado final"}
    H --> I
    I --> J["Bundle e informe"]
```

La recepción registra el trabajo y entrega su identificador. Un worker ejecuta las etapas posteriores de forma asíncrona. Las dos validaciones son independientes y determinan el estado del resultado.
