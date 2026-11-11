//! Configuración que cambia el contenido del bundle (plan 3, I3). `documento.md` y el
//! informe la declaran en `conversion_settings`, con su huella `settings_sha256`, para que
//! dos conversiones del mismo original puedan compararse: si la huella coincide, la
//! configuración es la misma. Solo cuentan los parámetros que cambian el bundle de ese
//! formato; los límites que rechazan el trabajo no.
//!
//! La huella es el SHA-256 de `conversion_settings` en forma canónica: JSON compacto, con
//! las claves de cada mapa ordenadas por sus bytes UTF-8, enteros en decimal y cadenas con
//! el escape de JSON. Se calcula sobre el frontmatter ya escrito y leído, igual que la
//! recalcula el validador.
use crate::bundle::hex;
use crate::format::Format;
use crate::output::Fields;
use crate::worker::Config;
use anyhow::{Result, bail};
use sha2::{Digest, Sha256};
use std::path::Path;
use yaml_serde::Value as YamlValue;

/// Valor de un archivo de configuración que no se encuentra.
const MISSING: &str = "ausente";

/// Configuración declarada de una conversión.
#[derive(Clone, Debug)]
pub(crate) struct Settings {
    values: Fields,
    /// Falso si el resultado depende del tiempo de ejecución (el OCR dejó páginas sin
    /// transcribir por agotar su plazo).
    reproducible: bool,
}

impl Settings {
    /// DOCX y XLSX: ningún parámetro cambia su bundle.
    pub(crate) fn none() -> Self {
        Settings {
            values: Fields::new(),
            reproducible: true,
        }
    }
    /// PPTX: la fuente con que se dibujan las vistas.
    pub(crate) fn pptx(font: &[u8]) -> Self {
        Settings {
            values: Fields::new().with("view_font_sha256", hex(&Sha256::digest(font))),
            reproducible: true,
        }
    }
    /// PDF: el límite de píxeles de las imágenes y, con el OCR activo, sus límites de
    /// páginas y de imágenes, el plazo del que sale su presupuesto de tiempo y la huella
    /// de los modelos.
    pub(crate) fn pdf(cfg: &Config) -> Self {
        let mut ocr = Fields::new().with("enabled", cfg.ocr.enabled);
        if cfg.ocr.enabled {
            ocr = ocr
                .with("max_pages", cfg.ocr.max_pages)
                .with("max_images", cfg.ocr.max_images)
                .with("timeout_s", cfg.timeout.as_secs());
            for (key, file) in [
                ("det_sha256", "det.onnx"),
                ("rec_sha256", "rec.onnx"),
                ("dict_sha256", "dict.txt"),
            ] {
                ocr = ocr.with(key, file_sha256(&cfg.ocr.models_dir.join(file)));
            }
        }
        Settings {
            values: Fields::new()
                .with("max_image_pixels", cfg.max_image_pixels)
                .with("ocr", ocr),
            reproducible: true,
        }
    }
    /// Marca la conversión como no reproducible.
    pub(crate) fn time_dependent(mut self, on: bool) -> Self {
        self.reproducible &= !on;
        self
    }
    /// Añade `conversion_settings`, `settings_sha256` y, si procede, `reproducible: false`.
    pub(crate) fn annotate(&self, fields: Fields) -> Fields {
        let fields = fields
            .with("conversion_settings", self.values.clone())
            .with("settings_sha256", self.sha256());
        if self.reproducible {
            fields
        } else {
            fields.with("reproducible", false)
        }
    }
    /// Huella canónica de los valores, calculada sobre su forma YAML leída.
    pub(crate) fn sha256(&self) -> String {
        let md = Fields::new()
            .with("conversion_settings", self.values.clone())
            .frontmatter();
        let fm = crate::bundle::frontmatter(&md).expect("frontmatter propio legible");
        digest(&fm["conversion_settings"]).expect("configuración representable")
    }
}

fn file_sha256(path: &Path) -> String {
    std::fs::read(path).map_or_else(|_| MISSING.into(), |b| hex(&Sha256::digest(&b)))
}

/// SHA-256 de un valor en forma canónica.
pub(crate) fn digest(v: &YamlValue) -> Result<String> {
    let mut out = String::new();
    canonical(v, &mut out)?;
    Ok(hex(&Sha256::digest(out.as_bytes())))
}

fn canonical(v: &YamlValue, out: &mut String) -> Result<()> {
    match v {
        YamlValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        YamlValue::Number(n) => match n.as_i64() {
            Some(i) => out.push_str(&i.to_string()),
            None => bail!("la configuración solo admite números enteros"),
        },
        YamlValue::String(s) => out.push_str(&serde_json::to_string(s)?),
        YamlValue::Sequence(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical(item, out)?;
            }
            out.push(']');
        }
        YamlValue::Mapping(m) => {
            let mut entries = Vec::with_capacity(m.len());
            for (k, v) in m {
                let Some(k) = k.as_str() else {
                    bail!("la configuración solo admite claves de texto")
                };
                entries.push((k, v));
            }
            entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            out.push('{');
            for (i, (k, v)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k)?);
                out.push(':');
                canonical(v, out)?;
            }
            out.push('}');
        }
        _ => bail!("la configuración solo admite mapas, listas, textos, enteros y booleanos"),
    }
    Ok(())
}

/// Claves de `conversion_settings` según el formato, en orden; las de `ocr` se comprueban
/// aparte porque dependen de que esté activo.
fn expected_keys(format: Format) -> &'static [&'static str] {
    match format {
        Format::Docx | Format::Xlsx => &[],
        Format::Pptx => &["view_font_sha256"],
        Format::Pdf => &["max_image_pixels", "ocr"],
    }
}

fn keys(v: &YamlValue) -> Option<Vec<&str>> {
    let mut k: Vec<&str> = v
        .as_mapping()?
        .keys()
        .map(|k| k.as_str())
        .collect::<Option<_>>()?;
    k.sort_unstable();
    Some(k)
}

fn is_sha256(v: Option<&YamlValue>) -> bool {
    v.and_then(YamlValue::as_str)
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

/// El documento y el informe declaran la misma configuración, con las claves de su
/// formato, y su huella coincide con la recalculada.
pub(crate) fn check(format: Format, document: &YamlValue, report: &YamlValue) -> Result<()> {
    for k in ["conversion_settings", "settings_sha256", "reproducible"] {
        if document.get(k) != report.get(k) {
            bail!("{k} difiere entre documento.md y el informe")
        }
    }
    let Some(settings) = document.get("conversion_settings") else {
        bail!("documento.md no declara conversion_settings")
    };
    if keys(settings).as_deref() != Some(expected_keys(format)) {
        bail!("conversion_settings no tiene las claves del formato {format:?}")
    }
    match format {
        Format::Pptx if !is_sha256(settings.get("view_font_sha256")) => {
            bail!("conversion_settings.view_font_sha256 no es una huella SHA-256")
        }
        Format::Pdf => check_pdf(settings, crate::version::declares_image_ocr(document))?,
        _ => {}
    }
    if document.get("settings_sha256").and_then(YamlValue::as_str)
        != Some(digest(settings)?.as_str())
    {
        bail!("settings_sha256 no corresponde a conversion_settings")
    }
    if document
        .get("reproducible")
        .is_some_and(|r| r.as_bool() != Some(false))
    {
        bail!("reproducible solo se declara, con false, cuando la conversión no lo es")
    }
    Ok(())
}

/// `images`: la configuración declara el límite de imágenes con OCR (perfil 1.18.0).
fn check_pdf(settings: &YamlValue, images: bool) -> Result<()> {
    if settings
        .get("max_image_pixels")
        .and_then(YamlValue::as_u64)
        .is_none()
    {
        bail!("conversion_settings.max_image_pixels no es un entero positivo")
    }
    let ocr = &settings["ocr"];
    let enabled = ocr.get("enabled").and_then(YamlValue::as_bool);
    // El límite de imágenes con OCR existe desde el perfil 1.18.0 (plan 4, Q1).
    let expected: &[&str] = match (enabled, images) {
        (Some(false), _) => &["enabled"],
        (Some(true), true) => &[
            "det_sha256",
            "dict_sha256",
            "enabled",
            "max_images",
            "max_pages",
            "rec_sha256",
            "timeout_s",
        ],
        (Some(true), false) => &[
            "det_sha256",
            "dict_sha256",
            "enabled",
            "max_pages",
            "rec_sha256",
            "timeout_s",
        ],
        (None, _) => bail!("conversion_settings.ocr.enabled no es un booleano"),
    };
    if keys(ocr).as_deref() != Some(expected) {
        bail!("conversion_settings.ocr no tiene las claves esperadas")
    }
    if enabled == Some(true) {
        for k in ["max_pages", "max_images", "timeout_s"] {
            if (k != "max_images" || images) && ocr.get(k).and_then(YamlValue::as_u64).is_none() {
                bail!("conversion_settings.ocr.{k} no es un entero")
            }
        }
        for k in ["det_sha256", "rec_sha256", "dict_sha256"] {
            let v = ocr.get(k);
            if !is_sha256(v) && v.and_then(YamlValue::as_str) != Some(MISSING) {
                bail!("conversion_settings.ocr.{k} no es una huella SHA-256 ni «{MISSING}»")
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frontmatter con la configuración, como el de un bundle del perfil actual.
    fn parsed(s: &Settings) -> YamlValue {
        let fields = Fields::new().with("profile_version", crate::version::PROFILE);
        crate::bundle::frontmatter(&s.annotate(fields).frontmatter()).unwrap()
    }

    #[test]
    fn canonical_form_sorts_keys_and_is_compact() {
        let v: YamlValue = yaml_serde::from_str("b: 1\na: {d: \"x\\\"y\", c: true}").unwrap();
        let mut out = String::new();
        canonical(&v, &mut out).unwrap();
        assert_eq!(out, r#"{"a":{"c":true,"d":"x\"y"},"b":1}"#);
        // Huella de un mapa vacío, la de DOCX y XLSX: SHA-256 de `{}`.
        assert_eq!(
            Settings::none().sha256(),
            "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        );
        let float: YamlValue = yaml_serde::from_str("a: 1.5").unwrap();
        assert!(digest(&float).is_err());
    }

    #[test]
    fn the_hash_changes_with_any_setting_and_only_then() {
        let cfg = Config::default();
        let base = Settings::pdf(&cfg).sha256();
        assert_eq!(base, Settings::pdf(&cfg).sha256());
        let mut ocr = cfg.clone();
        ocr.ocr.enabled = true;
        ocr.ocr.models_dir = "/nonexistent/okf-ocr".into();
        let with_ocr = Settings::pdf(&ocr).sha256();
        assert_ne!(base, with_ocr);
        let mut pages = ocr.clone();
        pages.ocr.max_pages += 1;
        assert_ne!(with_ocr, Settings::pdf(&pages).sha256());
        let mut images = ocr.clone();
        images.ocr.max_images += 1;
        assert_ne!(with_ocr, Settings::pdf(&images).sha256());
        let mut pixels = cfg.clone();
        pixels.max_image_pixels -= 1;
        assert_ne!(base, Settings::pdf(&pixels).sha256());
        // Lo que no cambia el bundle PDF no entra en la huella.
        let mut other = cfg.clone();
        other.max_slides += 1;
        other.timeout *= 2;
        assert_eq!(base, Settings::pdf(&other).sha256());
        assert_ne!(Settings::pptx(b"a").sha256(), Settings::pptx(b"b").sha256());
    }

    #[test]
    fn the_validator_recomputes_and_checks_the_keys_of_each_format() {
        let pdf = parsed(&Settings::pdf(&Config::default()));
        check(Format::Pdf, &pdf, &pdf).unwrap();
        assert!(check(Format::Docx, &pdf, &pdf).is_err());
        let mut altered = pdf.clone();
        altered["conversion_settings"]["max_image_pixels"] = 1.into();
        assert!(check(Format::Pdf, &altered, &altered).is_err());
        assert!(check(Format::Pdf, &pdf, &altered).is_err());
        let mut ocr = Config::default();
        ocr.ocr.enabled = true;
        ocr.ocr.models_dir = "/nonexistent/okf-ocr".into();
        let late = parsed(&Settings::pdf(&ocr).time_dependent(true));
        assert_eq!(late["reproducible"], YamlValue::Bool(false));
        check(Format::Pdf, &late, &late).unwrap();
        let mut claimed = late.clone();
        claimed["reproducible"] = true.into();
        assert!(check(Format::Pdf, &claimed, &claimed).is_err());
        // Antes del perfil 1.18.0, la configuración del OCR no declara el límite de
        // imágenes; desde él, lo exige.
        let mut previous = late.clone();
        previous["profile_version"] = "1.17.0".into();
        assert!(check(Format::Pdf, &previous, &previous).is_err());
        let ocr_settings = previous["conversion_settings"]["ocr"]
            .as_mapping_mut()
            .unwrap();
        ocr_settings.remove("max_images");
        let digest_now = digest(&previous["conversion_settings"]).unwrap();
        previous["settings_sha256"] = digest_now.into();
        check(Format::Pdf, &previous, &previous).unwrap();
        let mut current = previous.clone();
        current["profile_version"] = crate::version::PROFILE.into();
        assert!(check(Format::Pdf, &current, &current).is_err());
        let pptx = parsed(&Settings::pptx(b"fuente"));
        check(Format::Pptx, &pptx, &pptx).unwrap();
        let none = parsed(&Settings::none());
        check(Format::Xlsx, &none, &none).unwrap();
        let mut missing = none.clone();
        missing
            .as_mapping_mut()
            .unwrap()
            .remove("conversion_settings");
        assert!(check(Format::Xlsx, &missing, &missing).is_err());
    }
}
