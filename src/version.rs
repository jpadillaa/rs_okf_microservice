//! Identidad del productor y del contrato del bundle (plan 2, C9).
use crate::output::Fields;
use serde::{Deserialize, Serialize};

pub(crate) const PROFILE: &str = "1.24.0";
/// Perfiles anteriores cuyos bundles siguen siendo válidos: 1.1.0 añadió reglas para
/// páginas que un bundle 1.0.0 no puede contener (OCR del servicio), 1.2.0 solo cambia
/// qué guiones de fin de línea conserva el texto PDF, que el validador no comprueba,
/// 1.3.0 añade la configuración de la conversión, 1.4.0 sus identificadores, 1.5.0 la
/// `document_key` opcional, 1.6.0 las relaciones entre conceptos, 1.7.0 las referencias
/// internas resueltas o marcadas y el inventario de externas, 1.8.0 las cajas PDF y
/// los localizadores de forma PPTX, 1.9.0 el marcador del texto generado y 1.10.0 los
/// permisos recibidos (`access`), que los anteriores no declaran. 1.11.0 reconstruye
/// títulos PDF fragmentados. 1.12.0 reconstruye tablas por columnas estables y listados
/// con notas laterales, sin cambiar la validación de los bundles anteriores. 1.13.0 separa cabeceras
/// variables y reúne numeración lateral con su título confirmado. 1.14.0 conserva
/// vistas y navegación de exportaciones por diapositivas. 1.15.0 reconoce el OCR previo
/// invisible y 1.16.0 recupera listados de esa capa. 1.17.0 lleva los localizadores de
/// bloque PDF a su archivo `.spans.json`; los anteriores los tienen en el frontmatter.
/// 1.18.0 transcribe con OCR el texto de las imágenes de contenido y declara su límite.
/// 1.19.0 lee como columnas, y no como tabla, dos columnas de texto corrido (un índice),
/// sin cambiar la validación de los bundles anteriores. 1.20.0 lee por columnas, y no
/// como tabla, los rótulos de una figura con pie numerado. 1.21.0 no escribe como tabla
/// una sola fila lógica de celdas cuyas líneas se continúan. 1.22.0 no escribe un espacio
/// entre dos letras que se tocan (ligaduras expandidas, interletraje). 1.23.0 escribe
/// como una tabla el contenido formado por filas de número, título y página. 1.24.0 lleva
/// los localizadores de forma PPTX, de bloque DOCX y de región en línea y muestra XLSX a su
/// archivo `.spans.json`; los anteriores los tienen en el frontmatter.
const PREVIOUS_PROFILES: [&str; 24] = [
    "1.0.0", "1.1.0", "1.2.0", "1.3.0", "1.4.0", "1.5.0", "1.6.0", "1.7.0", "1.8.0", "1.9.0",
    "1.10.0", "1.11.0", "1.12.0", "1.13.0", "1.14.0", "1.15.0", "1.16.0", "1.17.0", "1.18.0",
    "1.19.0", "1.20.0", "1.21.0", "1.22.0", "1.23.0",
];
/// Perfiles anteriores al OCR de imágenes y a su límite (plan 4, Q1).
const WITHOUT_IMAGE_OCR: [&str; 18] = [
    "1.0.0", "1.1.0", "1.2.0", "1.3.0", "1.4.0", "1.5.0", "1.6.0", "1.7.0", "1.8.0", "1.9.0",
    "1.10.0", "1.11.0", "1.12.0", "1.13.0", "1.14.0", "1.15.0", "1.16.0", "1.17.0",
];
/// Perfiles anteriores a `conversion_settings` (plan 3, I3).
const WITHOUT_SETTINGS: [&str; 3] = ["1.0.0", "1.1.0", "1.2.0"];
/// Perfiles anteriores a `conversion_id` y `unit_id` (plan 3, I1).
const WITHOUT_IDS: [&str; 4] = ["1.0.0", "1.1.0", "1.2.0", "1.3.0"];
/// Perfiles anteriores a `parent`, `previous` y `next` (plan 3, I4).
const WITHOUT_RELATIONS: [&str; 6] = ["1.0.0", "1.1.0", "1.2.0", "1.3.0", "1.4.0", "1.5.0"];
pub(crate) const PACKAGE: &str = env!("CARGO_PKG_VERSION");
pub(crate) const COMMIT: &str = env!("OKF_BUILD_COMMIT");

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub(crate) struct Versions {
    /// Versión del paquete que produjo el bundle, o null antes de publicar.
    #[schema(required)]
    pub converter_version: Option<String>,
    /// Commit del conversor, `unknown` si no se conoce, o null antes de publicar.
    #[schema(required)]
    pub converter_commit: Option<String>,
    /// Versión del perfil documental del bundle, o null antes de publicar.
    #[schema(required)]
    pub profile_version: Option<String>,
}
impl Versions {
    pub(crate) fn current() -> Self {
        Self {
            converter_version: Some(PACKAGE.into()),
            converter_commit: Some(COMMIT.into()),
            profile_version: Some(PROFILE.into()),
        }
    }
    pub(crate) fn annotate(fields: Fields) -> Fields {
        fields
            .with("converter_version", PACKAGE)
            .with("converter_commit", COMMIT)
            .with("profile_version", PROFILE)
    }
    pub(crate) fn from_report(fm: &yaml_serde::Value) -> Self {
        let field = |k| {
            fm.get(k)
                .and_then(yaml_serde::Value::as_str)
                .map(str::to_owned)
        };
        Self {
            converter_version: field("converter_version"),
            converter_commit: field("converter_commit"),
            profile_version: field("profile_version"),
        }
    }
}

/// ¿Debe el bundle declarar `conversion_settings`? Desde el perfil 1.3.0; un bundle
/// heredado sin versión tampoco la tiene.
pub(crate) fn declares_settings(document: &yaml_serde::Value) -> bool {
    document
        .get("profile_version")
        .and_then(yaml_serde::Value::as_str)
        .is_some_and(|v| !WITHOUT_SETTINGS.contains(&v))
}

/// ¿Debe el bundle declarar `conversion_id` y `unit_id`? Desde el perfil 1.4.0.
pub(crate) fn declares_ids(document: &yaml_serde::Value) -> bool {
    document
        .get("profile_version")
        .and_then(yaml_serde::Value::as_str)
        .is_some_and(|v| !WITHOUT_IDS.contains(&v))
}

/// ¿Declara la configuración del OCR su límite de imágenes? Desde el perfil 1.18.0.
pub(crate) fn declares_image_ocr(document: &yaml_serde::Value) -> bool {
    document
        .get("profile_version")
        .and_then(yaml_serde::Value::as_str)
        .is_some_and(|v| !WITHOUT_IMAGE_OCR.contains(&v))
}

/// ¿Debe el bundle declarar las relaciones entre conceptos? Desde el perfil 1.6.0.
pub(crate) fn declares_relations(document: &yaml_serde::Value) -> bool {
    document
        .get("profile_version")
        .and_then(yaml_serde::Value::as_str)
        .is_some_and(|v| !WITHOUT_RELATIONS.contains(&v))
}

/// Reconoce el perfil previo al versionado; para bundles versionados, ambos registros
/// deben coincidir y este binario debe conocer el contrato declarado.
pub(crate) fn check(
    document: &yaml_serde::Value,
    report: &yaml_serde::Value,
) -> anyhow::Result<()> {
    let keys = ["converter_version", "converter_commit", "profile_version"];
    if keys
        .iter()
        .all(|k| document.get(k).is_none() && report.get(k).is_none())
    {
        return Ok(());
    }
    let doc = Versions::from_report(document);
    let rep = Versions::from_report(report);
    if doc != rep
        || keys.iter().any(|k| {
            document
                .get(k)
                .and_then(yaml_serde::Value::as_str)
                .is_none_or(str::is_empty)
        })
    {
        anyhow::bail!(
            "versiones del conversor o del perfil ausentes o incoherentes entre documento e informe"
        )
    }
    let known = |v: &str| v == PROFILE || PREVIOUS_PROFILES.contains(&v);
    if !doc.profile_version.as_deref().is_some_and(known) {
        anyhow::bail!("versión del perfil no admitida: {:?}", doc.profile_version)
    }
    let commit = doc.converter_commit.as_deref().unwrap();
    if commit != "unknown" && !(commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        anyhow::bail!("commit del conversor inválido")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versioned_contract_rejects_mismatches_and_unknown_profiles() {
        let v = serde_json::to_string(&Versions::current()).unwrap();
        let doc: yaml_serde::Value = yaml_serde::from_str(&v).unwrap();
        check(&doc, &doc).unwrap();
        let mut report = doc.clone();
        report["converter_version"] = "999".into();
        assert!(check(&doc, &report).is_err());
        for v in [
            "1.0.0", "1.1.0", "1.2.0", "1.3.0", "1.4.0", "1.5.0", "1.6.0", "1.7.0", "1.8.0",
            "1.9.0", "1.10.0", "1.11.0", "1.12.0", "1.13.0", "1.14.0", "1.15.0", "1.16.0",
            "1.17.0",
        ] {
            let mut previous = doc.clone();
            previous["profile_version"] = v.into();
            check(&previous, &previous).unwrap();
        }
        let mut future = doc.clone();
        future["profile_version"] = "999".into();
        assert!(check(&future, &future).is_err());
        let legacy: yaml_serde::Value = yaml_serde::from_str("type: antiguo").unwrap();
        check(&legacy, &legacy).unwrap();
        assert!(check(&legacy, &doc).is_err());
    }
}
