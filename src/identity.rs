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
use crate::output::Fields;
use crate::settings::Settings;
use anyhow::{Result, bail};
use yaml_serde::Value as YamlValue;

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

/// Configuración e identificador de la conversión para `documento.md` y el informe.
pub(crate) fn annotate(fields: Fields, source_sha256: &str, settings: &Settings) -> Fields {
    settings
        .annotate(fields)
        .with("conversion_id", conversion_id(source_sha256, settings))
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
}
