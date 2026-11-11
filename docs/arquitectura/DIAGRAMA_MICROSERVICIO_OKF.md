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

## Procesos y frontera de aislamiento

Las etapas anteriores se reparten entre tres procesos. Solo el proceso de conversión abre el documento, y es el único sin red ni credenciales.

```mermaid
flowchart LR
    CLI["Cliente"] <--> API
    subgraph RED["Con acceso a PostgreSQL por la red interna"]
        API["API<br/>Recepción y registro<br/>consulta y descarga"]
        SUP["Worker (supervisor)<br/>reclama, vigila<br/>y publica"]
    end
    subgraph AISLADO["Sin red ni credenciales"]
        CONV["okf-microservice convert<br/>inspección, extracción, OCR,<br/>reconciliación, ensamblado<br/>y validación"]
    end
    DB[("PostgreSQL<br/>trabajos")]
    FS[("Almacén<br/>originales, intentos<br/>y publicados")]
    API <--> DB
    SUP <--> DB
    API <--> FS
    SUP <--> FS
    SUP -- "lanza un hijo por trabajo" --> CONV
    CONV -- "resultado y bundle.zip" --> SUP
    CONV -. "solo el original y<br/>su carpeta de intento" .-> FS
```

El detalle de esta frontera (qué la cruza, el orden de las capas, el motivo de cada una y las diferencias entre Linux y macOS) está en la [arquitectura técnica, Frontera del proceso de conversión](ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#frontera-del-proceso-de-conversión-plan-2-c3-y-c4). La derivación de `conversion_id` y `unit_id`, y su diferencia con `job_id`, está en [Identificadores de la conversión y de sus unidades](ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#identificadores-de-la-conversión-y-de-sus-unidades-plan-3-i1).
