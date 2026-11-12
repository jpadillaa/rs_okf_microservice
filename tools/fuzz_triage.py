#!/usr/bin/env python3
"""Decide si un fallo del fuzzing nocturno es solo un defecto ya conocido (plan 2, C5).

Uso: fuzz_triage.py FORMATO LOG. Termina con 0 si el log de cargo-fuzz acaba en un pánico
cuya ubicación está en KNOWN_PANICS para ese formato y no muestra ningún otro error; con 1
en cualquier otro caso. La CI lo usa para no dejar en rojo una noche que solo reencuentra
un defecto abierto: el caso se sigue subiendo como artefacto y la noche cuenta hasta él.
"""
from pathlib import Path
import re
import sys

# Ubicación del pánico, con la versión de la dependencia en la ruta: al actualizarla deja
# de coincidir y la noche vuelve a fallar si el defecto persiste. Se quita de aquí, junto
# con su entrada de OPEN en tools/fuzz_seeds.py, cuando la corrección llega.
KNOWN_PANICS = {
    # Corte de un `str` dentro de U+FFFD en has_standalone_obj_keyword (pdf_oxide#2995),
    # ver docs/decisiones/FUZZING.md#primera-campaña-completa-y-hallazgo-pendiente.
    "pdf": {"pdf_oxide-0.3.78/src/document.rs:25363:"},
}
# Ubicación dentro de una dependencia del registro: `<crate>-<versión>/src/<archivo>:<línea>:`.
PANIC = re.compile(r"panicked at \S*/([\w-]+-\d+\.\d+\.\d+[^/\s]*/src/\S+?:\d+:)\d+:")
# Un pánico de Rust aborta el proceso y libFuzzer lo informa como «deadly signal».
# Cualquier otro informe (ASan, memoria, tiempo, fuga) es un fallo distinto.
ERROR = re.compile(r"==\d+== ?ERROR: (.+)")


def known_failure(fmt: str, log: str) -> bool:
    panics = PANIC.findall(log)
    return (
        bool(panics)
        and set(panics) <= KNOWN_PANICS.get(fmt, set())
        and ERROR.findall(log) == ["libFuzzer: deadly signal"]
    )


if __name__ == "__main__":
    fmt, path = sys.argv[1], Path(sys.argv[2])
    log = path.read_text(errors="replace") if path.exists() else ""
    if known_failure(fmt, log):
        print(f"{fmt}: el fallo es un defecto conocido ({', '.join(sorted(set(PANIC.findall(log))))})")
        sys.exit(0)
    print(f"{fmt}: el fallo no es un defecto conocido")
    sys.exit(1)
