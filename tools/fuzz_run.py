#!/usr/bin/env python3
"""Fuzzea un formato durante un presupuesto de tiempo y registra el resultado (plan 2, C5).

Uso: fuzz_run.py FORMATO --minutos N [--binario RUTA] [-- OPCIONES DE LIBFUZZER...]

Ejecuta el objetivo ya compilado (`cargo fuzz build FORMATO`) sobre fuzz/corpus/FORMATO.
Si una ejecución termina en un defecto conocido (tools/fuzz_triage.py), conserva el caso,
acota el corpus y vuelve a empezar con el tiempo que quede. Mide la carga del corpus y la
mutación (desde la línea INITED), escribe fuzz/FORMATO.log y fuzz/FORMATO-resultado.json, y
añade una fila al resumen de la ejecución de Actions. Termina con el código del fallo si
aparece uno que no es conocido.
"""
from datetime import datetime, timezone
from pathlib import Path
import argparse
import json
import os
import re
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from fuzz_seeds import RESTORED_LIMITS, prune_restored  # noqa: E402
from fuzz_triage import PANIC, known_failure  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
# Un intento que no tiene al menos este margen no llegaría a mutar tras cargar el corpus.
MIN_ATTEMPT_S = 180
# Tope de reinicios por defecto conocido, por si uno aparece justo después de cada carga.
MAX_KNOWN = 30
STATUS = re.compile(r"^#(\d+)\s+(INITED|NEW|REDUCE|pulse|DONE|RELOAD)\b")


def attempt(cmd: list[str], log) -> tuple[int, str, float, float, int]:
    """Ejecuta libFuzzer una vez; devuelve código, salida, carga, mutación y ejecuciones."""
    start = time.monotonic()
    inited_at = None
    inited_n = last_n = 0
    lines = []
    with subprocess.Popen(
        cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace"
    ) as proc:
        for line in proc.stdout:
            sys.stdout.write(line)
            sys.stdout.flush()
            log.write(line)
            lines.append(line)
            if m := STATUS.match(line):
                last_n = int(m.group(1))
                if m.group(2) == "INITED" and inited_at is None:
                    inited_at, inited_n = time.monotonic(), last_n
    end = time.monotonic()
    load = (inited_at or end) - start
    mutation = end - inited_at if inited_at else 0.0
    runs = last_n - inited_n if inited_at else 0
    return proc.returncode, "".join(lines), load, mutation, runs


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("formato")
    parser.add_argument("--minutos", type=float, required=True)
    parser.add_argument("--binario", type=Path)
    parser.add_argument("libfuzzer", nargs="*")
    args = parser.parse_args()
    fmt = args.formato
    binary = args.binario
    if binary is None:
        found = sorted((ROOT / "fuzz" / "target").glob(f"*/release/{fmt}"))
        if len(found) != 1:
            sys.exit(f"no se encontró un único binario de {fmt}; compílalo con cargo fuzz build {fmt}")
        binary = found[0]
    corpus = ROOT / "fuzz" / "corpus" / fmt
    artifacts = ROOT / "fuzz" / "artifacts" / fmt
    artifacts.mkdir(parents=True, exist_ok=True)

    budget = args.minutos * 60
    deadline = time.monotonic() + budget
    load = mutation = 0.0
    runs = known = 0
    code, result = 0, "sin fallos"
    panics: set[str] = set()
    with open(ROOT / "fuzz" / f"{fmt}.log", "w") as log:
        while (remaining := int(deadline - time.monotonic())) >= MIN_ATTEMPT_S:
            cmd = [str(binary), f"-artifact_prefix={artifacts}/", f"-max_total_time={remaining}"]
            cmd += [*args.libfuzzer, str(corpus)]
            rc, output, a_load, a_mutation, a_runs = attempt(cmd, log)
            load, mutation, runs = load + a_load, mutation + a_mutation, runs + a_runs
            if rc == 0:
                break
            # Un defecto conocido que aparece antes de mutar volvería a aparecer en cada
            # intento: se trata como fallo para que no consuma el presupuesto en vano.
            if a_mutation > 0 and known_failure(fmt, output):
                known += 1
                panics.update(PANIC.findall(output))
                result = "defecto conocido"
                if fmt in RESTORED_LIMITS:
                    prune_restored(corpus, *RESTORED_LIMITS[fmt])
                if known >= MAX_KNOWN:
                    break
                continue
            code, result = rc or 1, "fallo"
            break

    record = {
        "formato": fmt,
        "fecha": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "ejecucion": os.environ.get("GITHUB_RUN_ID"),
        "rama": os.environ.get("GITHUB_REF_NAME"),
        "commit": os.environ.get("GITHUB_SHA"),
        "presupuesto_s": round(budget),
        "carga_s": round(load),
        "mutacion_s": round(mutation),
        "ejecuciones": runs,
        "defectos_conocidos": known,
        "ubicaciones_conocidas": sorted(panics),
        "resultado": result,
    }
    (ROOT / "fuzz" / f"{fmt}-resultado.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps(record, ensure_ascii=False))
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary, "a") as out:
            out.write(
                "| Formato | Resultado | Mutación | Carga | Ejecuciones | Defectos conocidos |\n"
                "| --- | --- | ---: | ---: | ---: | ---: |\n"
                f"| {fmt} | {result} | {mutation / 60:.1f} min | {load / 60:.1f} min "
                f"| {runs} | {known} |\n"
            )
    if known and code == 0:
        print(
            f"::warning title=fuzz ({fmt})::{known} intento(s) terminaron en un defecto "
            f"conocido ({', '.join(sorted(panics))}); ver tools/fuzz_triage.py"
        )
    return code


if __name__ == "__main__":
    sys.exit(main())
