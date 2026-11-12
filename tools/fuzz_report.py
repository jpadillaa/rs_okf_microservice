#!/usr/bin/env python3
"""Acumula el tiempo de mutación de cada formato hacia las dos horas de C5 (plan 2).

Uso: fuzz_report.py [--repo DUEÑO/REPOSITORIO] [--todas-las-ramas]

Lee con `gh` los artefactos `fuzz-resultado` que deja el fuzzing semanal (uno por
ejecución, con el JSON de tools/fuzz_run.py), les suma lo acumulado antes de la rotación
semanal (ANTERIOR) e imprime una tabla en Markdown. Los artefactos caducan a los 90 días;
el registro que perdura es la tabla de docs/decisiones/FUZZING.md, que se actualiza con
esta salida.
"""
from pathlib import Path
from tempfile import TemporaryDirectory
from zipfile import ZipFile
import argparse
import json
import subprocess

GOAL_S = 2 * 60 * 60
ORDER = ("xlsx", "pptx", "docx", "pdf")
# Mutación sin fallos nuevos del 6 al 12 de noviembre de 2026, medida en los logs de las
# ejecuciones nocturnas y manuales. XLSX empieza en cero: aquellas noches usaban semillas
# con fórmulas, fuera del alcance acordado (solo libros estáticos).
ANTERIOR = {"xlsx": 0, "pptx": 69, "docx": 1796, "pdf": 1066}


def gh(*args: str) -> str:
    return subprocess.run(["gh", *args], check=True, capture_output=True, text=True).stdout


def results(repo: str, all_branches: bool) -> list[dict]:
    pages = gh("api", "--paginate", f"repos/{repo}/actions/artifacts?name=fuzz-resultado&per_page=100",
               "--jq", ".artifacts[] | select(.expired | not) | @json")
    found = []
    with TemporaryDirectory() as tmp:
        for line in pages.splitlines():
            artifact = json.loads(line)
            if not all_branches and artifact["workflow_run"]["head_branch"] != "main":
                continue
            archive = Path(tmp) / f"{artifact['id']}.zip"
            archive.write_bytes(subprocess.run(
                ["gh", "api", f"repos/{repo}/actions/artifacts/{artifact['id']}/zip"],
                check=True, capture_output=True).stdout)
            with ZipFile(archive) as z:
                for name in z.namelist():
                    if name.endswith("-resultado.json"):
                        found.append(json.loads(z.read(name)))
    return sorted(found, key=lambda r: r["fecha"])


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", default=gh("repo", "view", "--json", "nameWithOwner",
                                             "--jq", ".nameWithOwner").strip())
    parser.add_argument("--todas-las-ramas", action="store_true")
    args = parser.parse_args()
    runs = results(args.repo, args.todas_las_ramas)

    print("| Formato | Semanas | Mutación acumulada | Objetivo | Último resultado | Fallos nuevos |")
    print("| --- | ---: | ---: | --- | --- | ---: |")
    for fmt in ORDER:
        mine = [r for r in runs if r["formato"] == fmt]
        total = ANTERIOR[fmt] + sum(r["mutacion_s"] for r in mine)
        failures = sum(r["resultado"] == "fallo" for r in mine)
        last = f"{mine[-1]['resultado']} ({mine[-1]['fecha'][:10]})" if mine else "—"
        goal = "cumplido" if total >= GOAL_S and not failures else f"faltan {max(GOAL_S - total, 0) / 60:.0f} min"
        print(f"| {fmt} | {len(mine)} | {total / 60:.1f} min | {goal} | {last} | {failures} |")


if __name__ == "__main__":
    main()
