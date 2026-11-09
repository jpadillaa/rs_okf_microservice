//! Identificadores de la conversión y de sus unidades (plan 3, I1).
//!
//! `conversion_id` identifica una conversión por lo que determina su contenido: la
//! huella del original, el perfil y la configuración (`settings_sha256`). Es el SHA-256
//! de esos tres valores en la forma canónica de la configuración. No incluye la versión
//! ni el commit del conversor: un cambio que altera el bundle sube el perfil y, con él,
//! el identificador; uno que no lo altera conserva los identificadores.
//!
//! Cada concepto declara `unit_id`, que es `conversion_id`, una barra y su ruta en el
//! bundle. Un pasaje se identifica con `unit_id` y su `source_spans[].id`.
//!
//! `document_key` (plan 3, I2) es la clave opaca con que el sistema de origen agrupa las
//! versiones de un documento. El servicio la recibe con el original, la conserva sin
//! interpretarla y no la usa en `conversion_id`: no cambia el contenido.
use crate::output::Fields;
use crate::settings::Settings;
use anyhow::{Result, bail};
use yaml_serde::Value as YamlValue;

/// Longitud máxima de `document_key`, en caracteres.
pub(crate) const MAX_KEY_LEN: usize = 256;

/// ¿Es `key` una `document_key` admitida? De 1 a 256 caracteres ASCII visibles (sin
/// espacios ni controles); el sistema de origen codifica cualquier otra clave.
pub(crate) fn valid_key(key: &str) -> bool {
    (1..=MAX_KEY_LEN).contains(&key.len()) && key.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

/// Original de una conversión: su huella y, si se recibieron, su `document_key` y sus
/// permisos (`access`, plan 3, I7).
#[derive(Clone, Debug)]
pub(crate) struct Source {
    pub(crate) sha256: String,
    pub(crate) document_key: Option<String>,
    /// JSON recibido con el original, sin interpretar ni reescribir.
    pub(crate) access: Option<String>,
}

/// ¿Es `access` un JSON válido? El servicio no interpreta su contenido.
pub(crate) fn valid_access(access: &str) -> bool {
    serde_json::from_str::<serde::de::IgnoredAny>(access).is_ok()
}

impl Source {
    /// Original sin `document_key`.
    pub(crate) fn new(sha256: &str) -> Self {
        Source {
            sha256: sha256.to_owned(),
            document_key: None,
            access: None,
        }
    }
}

/// Identificador de una conversión con el perfil actual.
pub(crate) fn conversion_id(source_sha256: &str, settings: &Settings) -> String {
    compute(crate::version::PROFILE, &settings.sha256(), source_sha256)
}

fn compute(profile: &str, settings_sha256: &str, source_sha256: &str) -> String {
    let mut m = yaml_serde::Mapping::new();
    m.insert("profile_version".into(), profile.into());
    m.insert("settings_sha256".into(), settings_sha256.into());
    m.insert("source_sha256".into(), source_sha256.into());
    crate::settings::digest(&YamlValue::Mapping(m)).expect("valores de texto")
}

/// Identificador de la unidad que ocupa `path` en el bundle.
pub(crate) fn unit_id(conversion_id: &str, path: &str) -> String {
    format!("{conversion_id}/{path}")
}

/// Configuración, identificador de la conversión y, si existe, `document_key` para
/// `documento.md` y el informe.
pub(crate) fn annotate(fields: Fields, source: &Source, settings: &Settings) -> Fields {
    settings
        .annotate(fields)
        .with("conversion_id", conversion_id(&source.sha256, settings))
        .with_opt("document_key", source.document_key.as_deref())
        .with_opt("access", source.access.as_deref())
}

/// `documento.md` y el informe declaran la misma `document_key`, o ninguno la declara,
/// y es una clave admitida.
pub(crate) fn check_key(document: &YamlValue, report: &YamlValue) -> Result<()> {
    for k in ["document_key", "access"] {
        if document.get(k) != report.get(k) {
            bail!("{k} difiere entre documento.md y el informe")
        }
    }
    if let Some(access) = document.get("access")
        && !access.as_str().is_some_and(valid_access)
    {
        bail!("access no es un JSON válido")
    }
    match document.get("document_key") {
        None => Ok(()),
        Some(k) if k.as_str().is_some_and(valid_key) => Ok(()),
        Some(_) => bail!("document_key no es una clave admitida"),
    }
}

/// `documento.md` y el informe declaran el mismo `conversion_id`, que corresponde a su
/// perfil, su configuración y la huella `source_sha256` del original incluido.
pub(crate) fn check(
    document: &YamlValue,
    report: &YamlValue,
    source_sha256: &str,
) -> Result<String> {
    let text = |v: &YamlValue, k: &str| v.get(k).and_then(YamlValue::as_str).map(str::to_owned);
    let declared = text(document, "conversion_id");
    if declared.is_none() || declared != text(report, "conversion_id") {
        bail!("conversion_id ausente o distinto entre documento.md y el informe")
    }
    let (Some(profile), Some(settings)) = (
        text(document, "profile_version"),
        text(document, "settings_sha256"),
    ) else {
        bail!("faltan los datos de los que sale conversion_id")
    };
    let expected = compute(&profile, &settings, source_sha256);
    if declared.as_deref() != Some(expected.as_str()) {
        bail!("conversion_id no corresponde al original, el perfil y la configuración")
    }
    Ok(expected)
}

/// El concepto `name` declara el `unit_id` que le corresponde.
pub(crate) fn check_unit(conversion_id: &str, name: &str, fm: &YamlValue) -> Result<()> {
    if fm.get("unit_id").and_then(YamlValue::as_str) != Some(unit_id(conversion_id, name).as_str())
    {
        bail!("{name}: unit_id ausente o distinto de conversion_id/{name}")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identifier_depends_on_the_original_the_profile_and_the_settings() {
        let none = Settings::none();
        let a = conversion_id(&"a".repeat(64), &none);
        assert_eq!(a, conversion_id(&"a".repeat(64), &none));
        assert_eq!(a.len(), 64);
        assert_ne!(a, conversion_id(&"b".repeat(64), &none));
        assert_ne!(
            a,
            conversion_id(&"a".repeat(64), &Settings::pptx(b"fuente"))
        );
        assert_ne!(
            a,
            compute("1.3.0", &none.sha256(), &"a".repeat(64)),
            "otro perfil, otro identificador"
        );
        // Forma canónica: las tres claves ordenadas, en JSON compacto.
        let canonical = format!(
            r#"{{"profile_version":"{}","settings_sha256":"{}","source_sha256":"{}"}}"#,
            crate::version::PROFILE,
            none.sha256(),
            "a".repeat(64)
        );
        use sha2::{Digest, Sha256};
        assert_eq!(a, crate::bundle::hex(&Sha256::digest(canonical.as_bytes())));
        assert_eq!(
            unit_id(&a, "contenido/texto.md"),
            format!("{a}/contenido/texto.md")
        );
    }

    #[test]
    fn document_keys_are_short_visible_ascii_and_must_agree() {
        for ok in ["a", "sp:sites/finanzas/123", &"x".repeat(MAX_KEY_LEN)] {
            assert!(valid_key(ok), "{ok}");
        }
        for bad in [
            "",
            "con espacio",
            "línea",
            "tab\t",
            &"x".repeat(MAX_KEY_LEN + 1),
        ] {
            assert!(!valid_key(bad), "{bad}");
        }
        let with: YamlValue = yaml_serde::from_str("document_key: \"k-1\"").unwrap();
        let without: YamlValue = yaml_serde::from_str("type: x").unwrap();
        check_key(&with, &with).unwrap();
        check_key(&without, &without).unwrap();
        assert!(check_key(&with, &without).is_err());
        let bad: YamlValue = yaml_serde::from_str("document_key: \"a b\"").unwrap();
        assert!(check_key(&bad, &bad).is_err());
    }
}
