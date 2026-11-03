#!/usr/bin/env python3
"""Copia ejemplos y regresiones sintéticas como semillas de cargo-fuzz (plan 2, C5)."""
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parent.parent
for fmt in ("pptx", "docx", "xlsx", "pdf"):
    target = ROOT / "fuzz" / "corpus" / fmt
    target.mkdir(parents=True, exist_ok=True)
    for doc in sorted((ROOT / "examples" / fmt).rglob(f"*.{fmt}")):
        if "esperado" in doc.parts or "cobertura" in doc.parts:
            continue
        relative = doc.relative_to(ROOT / "examples" / fmt)
        shutil.copyfile(doc, target / "__".join(relative.parts))
    for doc in sorted((ROOT / "fuzz" / "regressions" / fmt).glob(f"*.{fmt}")):
        shutil.copyfile(doc, target / f"regression__{doc.name}")
    print(f"{fmt}: {len(list(target.iterdir()))} semillas disponibles")
