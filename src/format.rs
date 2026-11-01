//! Formatos de entrada admitidos y su adaptador. Cada formato convierte un original
//! en un bundle y añade sus propias comprobaciones al validador común.
use crate::worker::{Config, Rejected};
use anyhow::Result;
use std::{
    fs,
    path::{Path, PathBuf},
};
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
    /// Convierte el original en el directorio del intento; devuelve el bundle y su estado.
    pub(crate) fn convert(
        self,
        cfg: &Config,
        bytes: &[u8],
        filename: &str,
        hash: &str,
        dir: &Path,
    ) -> Result<(PathBuf, String)> {
        match self {
            Format::Pptx => crate::pptx::convert(cfg, bytes, filename, hash, dir),
            Format::Docx => crate::docx::convert(cfg, bytes, filename, hash, dir),
            Format::Xlsx => crate::xlsx::convert(cfg, bytes, filename, hash, dir),
            Format::Pdf => crate::pdf::convert(cfg, bytes, filename, hash, dir),
        }
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
