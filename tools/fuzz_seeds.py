#!/usr/bin/env python3
"""Copia ejemplos y regresiones sintéticas como semillas de cargo-fuzz (plan 2, C5).

Uso: fuzz_seeds.py [FORMATO...]; sin argumentos, prepara los cuatro formatos.
"""
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile
import re
import shutil
import sys

ROOT = Path(__file__).resolve().parent.parent
FORMATS = ("pptx", "docx", "xlsx", "pdf")
# Regresiones de defectos aún abiertos. Como semilla harían caer el objetivo al cargar
# el corpus, sin llegar a mutar; su contención la comprueba tests/worker_process.rs.
# Se quitan de aquí, y de KNOWN_PANICS en tools/fuzz_triage.py, cuando la corrección
# llega a la dependencia.
OPEN = {"pdf/utf8-object-header.pdf"}
# Entradas que libFuzzer añadió al corpus en ejecuciones anteriores: tamaño máximo de cada
# una y suma máxima, en bytes. Las semillas no cuentan. libFuzzer ejecuta cada entrada al
# cargar el corpus. Con ASan, las mutaciones de p1.pdf (1,5 MB) tardan de segundos a más
# de un minuto: el 11-11-2026, 126 entradas PDF (43,7 MB) tardaron 8,4 min en cargar y la
# noche terminó sin mutar. Las PPTX tardan hasta 73 s por el renderizado: 445 entradas
# (44 MB) llenaron cada noche los 300 s. XLSX y DOCX cargan rápido, pero en cuatro minutos
# XLSX pasó de 3 entradas a 1439 (22 MB): el límite acota el artefacto que guarda el corpus.
# Ver docs/decisiones/FUZZING.md.
RESTORED_LIMITS = {
    "pdf": (128 * 1024, 4 * 1024 * 1024),
    "pptx": (128 * 1024, 4 * 1024 * 1024),
    "docx": (256 * 1024, 16 * 1024 * 1024),
    "xlsx": (256 * 1024, 16 * 1024 * 1024),
}
# libFuzzer nombra cada entrada que añade por el SHA-1 de su contenido.
LIBFUZZER_ENTRY = re.compile(r"[0-9a-f]{40}")
# El servicio solo se prueba con libros de contenido estático: texto, tablas de valores
# fijos y gráficos con sus datos ya calculados. Las semillas XLSX conservan el valor
# guardado de cada celda y pierden la fórmula; un libro con macros no es semilla.
FORMULA = re.compile(rb"<f\b[^>]*/>|<f\b[^>]*>.*?</f>", re.S)
MACRO_PARTS = re.compile(r"(?i)(^|/)(vbaProject\.bin|macrosheets/)")
CALC_CHAIN = re.compile(rb'<(Relationship|Override)\b[^>]*calcChain\.xml"[^>]*/>')


def prune_restored(target: Path, max_entry: int, budget: int) -> int:
    """Conserva las entradas añadidas por libFuzzer más pequeñas, dentro de los límites."""
    entries = sorted(
        (p.stat().st_size, p.name, p)
        for p in target.iterdir()
        if p.is_file() and LIBFUZZER_ENTRY.fullmatch(p.name)
    )
    total = removed = 0
    for size, _, path in entries:
        if size <= max_entry and total + size <= budget:
            total += size
        else:
            path.unlink()
            removed += 1
    return removed


def static_xlsx(source: Path, seed: Path) -> bool:
    """Escribe la versión estática del libro; devuelve False si tiene macros."""
    with ZipFile(source) as src:
        names = src.namelist()
        if any(MACRO_PARTS.search(n) for n in names):
            return False
        with ZipFile(seed, "w", ZIP_DEFLATED) as dst:
            for info in src.infolist():
                if info.filename == "xl/calcChain.xml":
                    continue
                data = src.read(info)
                if info.filename.startswith("xl/worksheets/") and info.filename.endswith(".xml"):
                    data = FORMULA.sub(b"", data)
                elif info.filename == "xl/workbook.xml":
                    data = re.sub(rb'\s(fullCalcOnLoad|forceFullCalc)="(1|true)"', b"", data)
                elif info.filename in ("[Content_Types].xml", "xl/_rels/workbook.xml.rels"):
                    data = CALC_CHAIN.sub(b"", data)
                dst.writestr(info, data)
    return True


def prepare(fmt: str) -> str:
    target = ROOT / "fuzz" / "corpus" / fmt
    target.mkdir(parents=True, exist_ok=True)
    removed = 0
    if fmt in RESTORED_LIMITS:
        removed = prune_restored(target, *RESTORED_LIMITS[fmt])
    for doc in sorted((ROOT / "examples" / fmt).rglob(f"*.{fmt}")):
        if "esperado" in doc.parts or "cobertura" in doc.parts:
            continue
        seed = target / "__".join(doc.relative_to(ROOT / "examples" / fmt).parts)
        if fmt == "xlsx":
            if not static_xlsx(doc, seed):
                seed.unlink(missing_ok=True)
        else:
            shutil.copyfile(doc, seed)
    for doc in sorted((ROOT / "fuzz" / "regressions" / fmt).glob(f"*.{fmt}")):
        seed = target / f"regression__{doc.name}"
        if f"{fmt}/{doc.name}" in OPEN:
            # También la retira de un corpus restaurado o de una ejecución local.
            seed.unlink(missing_ok=True)
            continue
        shutil.copyfile(doc, seed)
    files = list(target.iterdir())
    size = sum(p.stat().st_size for p in files)
    note = f"; {removed} entradas restauradas retiradas por tamaño" if removed else ""
    return f"{fmt}: {len(files)} semillas disponibles ({size / 1e6:.1f} MB){note}"


if __name__ == "__main__":
    for fmt in sys.argv[1:] or FORMATS:
        if fmt not in FORMATS:
            sys.exit(f"formato desconocido: {fmt}")
        print(prepare(fmt))
