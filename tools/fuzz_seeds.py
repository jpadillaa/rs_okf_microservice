#!/usr/bin/env python3
"""Copia ejemplos y regresiones sintéticas como semillas de cargo-fuzz (plan 2, C5)."""
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parent.parent
# Regresiones de defectos aún abiertos. Como semilla harían caer el objetivo al cargar
# el corpus, sin llegar a mutar; su contención la comprueba tests/worker_process.rs.
# Se quitan de aquí cuando la corrección llega a la dependencia.
OPEN = {"pdf/utf8-object-header.pdf"}

for fmt in ("pptx", "docx", "xlsx", "pdf"):
    target = ROOT / "fuzz" / "corpus" / fmt
    target.mkdir(parents=True, exist_ok=True)
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
    print(f"{fmt}: {len(list(target.iterdir()))} semillas disponibles")
