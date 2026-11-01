//! División en secciones compartida por DOCX y PDF (plan 1, R1). Cada formato detecta
//! sus encabezados y elige el nivel de división con sus propias reglas (especificación,
//! §5); comparten los tipos del resultado, los umbrales, el nombre de los conceptos y la
//! nota sobre el origen del título.
use crate::bundle::{Issue, truncate_chars};

/// Por debajo de este número de caracteres el documento es breve y forma un solo
/// concepto aunque tenga encabezados.
pub(crate) const BRIEF_MAX_CHARS: usize = 1500;
/// Longitud máxima de un párrafo sin estilo de encabezado que puede inferirse como tal.
pub(crate) const INFERRED_HEADING_MAX_CHARS: usize = 150;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Heading {
    /// Nivel desde 1.
    pub(crate) level: u8,
    /// Sin estilo, nivel de esquema ni marcador: inferido por tener el formato de un
    /// encabezado.
    pub(crate) inferred: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Section {
    pub(crate) title: String,
    /// Bloque o unidad del encabezado que abre la sección.
    pub(crate) heading: Option<usize>,
    /// Primer y último bloque o unidad (índices desde 0, inclusivos).
    pub(crate) from: usize,
    pub(crate) to: usize,
    /// Contenido anterior al primer encabezado de división.
    pub(crate) preamble: bool,
    pub(crate) inferred: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TitleSource {
    /// Párrafo con estilo «Title» (solo DOCX).
    Style,
    /// Primer encabezado, seguido directamente por otro de igual o mayor rango.
    Heading,
    /// Primer encabezado, único de su nivel y de rango superior a todos los demás.
    LoneHeading,
    Metadata,
    Filename,
}

/// Concepto `k` (desde 0) de un documento dividido por el nivel `split`, o el único
/// concepto de uno breve.
pub(crate) fn section_file(split: Option<u8>, k: usize) -> String {
    match split {
        None => "texto.md".into(),
        Some(_) => format!("seccion-{:03}.md", k + 1),
    }
}

/// Observación sobre el origen del título del documento. `lone` describe un primer
/// encabezado único de su nivel, que cada formato busca en un sitio distinto.
pub(crate) fn title_note(source: TitleSource, title: &str, lone: &str) -> Issue {
    Issue::note(format!(
        "Título del documento tomado {}: «{}»",
        match source {
            TitleSource::Style => "del párrafo con estilo Título",
            TitleSource::Heading =>
                "del primer encabezado, seguido directamente por otro de igual o mayor rango",
            TitleSource::LoneHeading => lone,
            TitleSource::Metadata => "de las propiedades del documento",
            TitleSource::Filename => "del nombre del archivo; el documento no lo declara",
        },
        truncate_chars(title, 120)
    ))
}
