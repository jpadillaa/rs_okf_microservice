#!/usr/bin/env python3
"""Copia ejemplos y regresiones sintéticas como semillas de cargo-fuzz (plan 2, C5)."""
from pathlib import Path
import re
import shutil

ROOT = Path(__file__).resolve().parent.parent
# Regresiones de defectos aún abiertos. Como semilla harían caer el objetivo al cargar
# el corpus, sin llegar a mutar; su contención la comprueba tests/worker_process.rs.
# Se quitan de aquí cuando la corrección llega a la dependencia.
OPEN = {"pdf/utf8-object-header.pdf"}
# Entradas que libFuzzer añadió al corpus en noches anteriores: tamaño máximo de cada una
# y suma máxima, en bytes. Las semillas no cuentan. libFuzzer ejecuta cada entrada al
# cargar el corpus, y las mutaciones PDF de p1.pdf (1,5 MB) tardan de segundos a más de
# un minuto con ASan: el 11-11-2026, 126 entradas (43,7 MB) tardaron 8,4 min en cargar y la
# noche terminó sin mutar. Ver docs/decisiones/FUZZING.md.
RESTORED_LIMITS = {"pdf": (128 * 1024, 4 * 1024 * 1024)}
# libFuzzer nombra cada entrada que añade por el SHA-1 de su contenido.
LIBFUZZER_ENTRY = re.compile(r"[0-9a-f]{40}")


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


for fmt in ("pptx", "docx", "xlsx", "pdf"):
    target = ROOT / "fuzz" / "corpus" / fmt
    target.mkdir(parents=True, exist_ok=True)
    removed = 0
    if fmt in RESTORED_LIMITS:
        removed = prune_restored(target, *RESTORED_LIMITS[fmt])
    for doc in sorted((ROOT / "examples" / fmt).rglob(f"*.{fmt}")):
        if "esperado" in doc.parts or "cobertura" in doc.parts:
            continue
        relative = doc.relative_to(ROOT / "examples" / fmt)
        shutil.copyfile(doc, target / "__".join(relative.parts))
    for doc in sorted((ROOT / "fuzz" / "regressions" / fmt).glob(f"*.{fmt}")):
        seed = target / f"regression__{doc.name}"
        if f"{fmt}/{doc.name}" in OPEN:
            # También la retira de un corpus restaurado de la caché o de una ejecución local.
            seed.unlink(missing_ok=True)
            continue
        shutil.copyfile(doc, seed)
    files = list(target.iterdir())
    size = sum(p.stat().st_size for p in files)
    note = f"; {removed} entradas restauradas retiradas por tamaño" if removed else ""
    print(f"{fmt}: {len(files)} semillas disponibles ({size / 1e6:.1f} MB){note}")
