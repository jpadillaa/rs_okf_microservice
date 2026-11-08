//! Validación independiente de un ZIP o directorio, sin extraer entradas no confiables.
use anyhow::{Context, Result, bail};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Component, Path},
};

pub(crate) fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('\\')
        && !name.contains(':')
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        && !name.split('/').any(|s| s == "." || s == "..")
}

pub(crate) fn path(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        bail!("el bundle no puede ser un enlace simbólico")
    }
    if path.is_dir() {
        let tmp = tempfile::tempdir()?;
        let packed = tmp.path().join("bundle.zip");
        crate::output::pack_directory(path, &packed)?;
        return self::path(&packed);
    }
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    let limits = crate::ooxml::limits();
    if zip.len() > limits.entries {
        bail!("demasiadas entradas en el bundle")
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        if !safe_name(entry.name()) || entry.is_symlink() || !names.insert(entry.name().to_owned())
        {
            bail!("ruta insegura o repetida en el bundle: {}", entry.name())
        }
        let declared = entry.size();
        if declared > limits.entry_bytes() {
            bail!("entrada del bundle demasiado grande")
        }
        total = total
            .checked_add(declared)
            .context("tamaño del bundle desbordado")?;
        if total > limits.total_mb.saturating_mul(1024 * 1024) {
            bail!("bundle expandido demasiado grande")
        }
        // Comprueba también el contenido real y su CRC antes de los lectores del perfil.
        let read = std::io::copy(
            &mut entry.take(limits.entry_bytes() + 1),
            &mut std::io::sink(),
        )?;
        if read != declared {
            bail!("tamaño real distinto del declarado en el bundle")
        }
    }
    drop(zip);
    crate::bundle::validate_bundle(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bundle::hex, format::Format, worker::Config};
    use sha2::{Digest, Sha256};

    #[test]
    fn all_examples_validate_as_zip_and_directory_and_reject_edits() {
        for doc in crate::golden::documents() {
            let tmp = tempfile::tempdir().unwrap();
            let bytes = fs::read(&doc).unwrap();
            let filename = doc.file_name().unwrap().to_str().unwrap();
            let built = Format::from_filename(filename)
                .unwrap()
                .convert(
                    &Config::default(),
                    &bytes,
                    filename,
                    &crate::identity::Source::new(&hex(&Sha256::digest(&bytes))),
                    tmp.path(),
                )
                .unwrap();
            path(&built.path).unwrap();
            let tree = tmp.path().join("tree");
            let mut zip = zip::ZipArchive::new(fs::File::open(&built.path).unwrap()).unwrap();
            for i in 0..zip.len() {
                let mut e = zip.by_index(i).unwrap();
                let target = tree.join(e.name());
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                std::io::copy(&mut e, &mut fs::File::create(target).unwrap()).unwrap();
            }
            path(&tree).unwrap_or_else(|e| panic!("{}: {e:#}", doc.display()));
            fs::write(tree.join("contenido/index.md"), "- [Ausente](ausente.md)\n").unwrap();
            assert!(
                path(&tree).is_err(),
                "{}: acepta enlace roto",
                doc.display()
            );
        }
    }

    #[test]
    fn directory_symlinks_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let tree = tmp.path().join("tree");
        fs::create_dir(&tree).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", tree.join("outside")).unwrap();
        assert!(path(&tree).unwrap_err().to_string().contains("regular"));
        assert!(!safe_name("../outside") && !safe_name("/etc/passwd") && !safe_name("a/../b"));
    }
}
