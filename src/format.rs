//! Formatos de entrada admitidos y su adaptador. Cada formato convierte un original
//! en un bundle y añade sus propias comprobaciones al validador común.
use crate::output::Built;
use crate::worker::{Config, Rejected};
use anyhow::Result;
use std::{fs, path::Path, time::Instant};
use zip::ZipArchive;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Format {
    Pptx,
    Docx,
    Xlsx,
    Pdf,
}

impl Format {
    pub(crate) const ALL: &[Format] = &[Format::Pptx, Format::Docx, Format::Xlsx, Format::Pdf];

    pub(crate) fn extension(self) -> &'static str {
        match self {
            Format::Pptx => "pptx",
            Format::Docx => "docx",
            Format::Xlsx => "xlsx",
            Format::Pdf => "pdf",
        }
    }
    pub(crate) fn from_extension(ext: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|f| f.extension().eq_ignore_ascii_case(ext))
    }
    pub(crate) fn from_filename(name: &str) -> Option<Self> {
        Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .and_then(Self::from_extension)
    }
    /// Ruta del original dentro del bundle.
    pub(crate) fn original_entry(self) -> String {
        format!("references/original.{}", self.extension())
    }
    /// Convierte el original en el directorio del intento; devuelve el bundle validado,
    /// su estado y la duración de sus etapas.
    pub(crate) fn convert(
        self,
        cfg: &Config,
        bytes: &[u8],
        filename: &str,
        source: &crate::identity::Source,
        dir: &Path,
    ) -> Result<Built> {
        let started = Instant::now();
        let mut built = match self {
            Format::Pptx => crate::pptx::convert(cfg, bytes, filename, source, dir),
            Format::Docx => crate::docx::convert(cfg, bytes, filename, source, dir),
            Format::Xlsx => crate::xlsx::convert(cfg, bytes, filename, source, dir),
            Format::Pdf => crate::pdf::convert(cfg, bytes, filename, source, dir),
        }?;
        let s = &mut built.stages;
        let total = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        s.read_ms = total.saturating_sub(s.assemble_ms + s.validate_ms);
        Ok(built)
    }
    /// Comprobaciones propias del formato sobre un bundle que ya superó las comunes.
    pub(crate) fn validate(
        self,
        z: &mut ZipArchive<fs::File>,
        names: &std::collections::HashSet<String>,
        original: &[u8],
    ) -> Result<()> {
        match self {
            Format::Pptx => crate::pptx::validate(z, names, original),
            Format::Docx => crate::docx::validate(z, names, original),
            Format::Xlsx => crate::xlsx::validate(z, names, original),
            Format::Pdf => crate::pdf::validate(z, names, original),
        }
    }
}

/// Formato de un original o rechazo permanente si no está admitido.
pub(crate) fn require(filename: &str) -> Result<Format> {
    Format::from_filename(filename)
        .ok_or_else(|| Rejected(format!("formato no admitido: {filename}; {}", supported())).into())
}

/// Texto para el usuario con las extensiones admitidas.
pub(crate) fn supported() -> String {
    let list: Vec<String> = Format::ALL
        .iter()
        .map(|f| format!(".{}", f.extension()))
        .collect();
    format!("esta entrega acepta {}", list.join(", "))
}
