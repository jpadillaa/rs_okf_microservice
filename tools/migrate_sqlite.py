#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["psycopg[binary]>=3.2,<4", "python-dotenv>=1,<2"]
# ///
"""Importación única de un registro SQLite antiguo; el servicio solo usa PostgreSQL."""
import argparse
import os
import sqlite3
import time
from pathlib import Path

import psycopg
from psycopg import sql
from dotenv import load_dotenv


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sqlite", type=Path)
    parser.add_argument("--env-file", type=Path, default=Path(".env"))
    parser.add_argument("--data-dir", type=Path, help="Directorio de los archivos del registro antiguo")
    parser.add_argument("--apply", action="store_true", help="Importar; sin esta opción solo se comprueba")
    args = parser.parse_args()
    if not args.env_file.is_file():
        parser.error("no existe el archivo de variables de entorno")
    load_dotenv(args.env_file, override=False)
    url = os.environ.get("OKF_DATABASE_URL")
    if not url:
        parser.error("falta OKF_DATABASE_URL")
    root = (args.data_dir or Path(os.environ.get("OKF_DATA_DIR", "data"))).resolve()
    # SQLite se abre explícitamente en solo lectura; nunca se borra ni se modifica.
    with sqlite3.connect(f"{args.sqlite.resolve().as_uri()}?mode=ro", uri=True) as source:
        source.row_factory = sqlite3.Row
        source.execute("BEGIN")
        jobs = [dict(row) for row in source.execute("SELECT * FROM jobs")]
        if any(row["status"] == "processing" for row in jobs):
            parser.error("hay trabajos processing: detén API/worker y termina o recupera sus asignaciones antes de importar")
        for row in jobs:
            for key in ("original", "bundle"):
                value = row.get(key)
                if not value:
                    continue
                path = Path(value)
                if path.is_absolute() or ".." in path.parts or not (root / path).resolve().is_relative_to(root):
                    parser.error("el registro contiene una ruta fuera del almacén")
                if row["status"] != "expired" and not (root / path).is_file():
                    parser.error("faltan archivos referenciados: revisa --data-dir")
        tables = {row[0] for row in source.execute("SELECT name FROM sqlite_master WHERE type='table'")}
        metrics = [dict(row) for row in source.execute("SELECT * FROM conversion_metrics")] if "conversion_metrics" in tables else []
    with psycopg.connect(url) as target:
        if target.execute("SELECT to_regclass('jobs')").fetchone()[0] is None:
            parser.error("inicializa primero el esquema arrancando la API nueva y detenla antes de importar")
        target.execute("LOCK TABLE jobs, conversion_metrics, worker_heartbeats IN ACCESS EXCLUSIVE MODE")
        if any(target.execute(sql.SQL("SELECT EXISTS(SELECT 1 FROM {})").format(sql.Identifier(table))).fetchone()[0]
               for table in ("jobs", "conversion_metrics", "worker_heartbeats")):
            parser.error("la base destino debe estar vacía y sin workers activos")
        columns = [r[0] for r in target.execute("SELECT column_name FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='jobs' ORDER BY ordinal_position")]
        for row in jobs:
            if row["status"] in ("complete", "partial", "failed") and not row.get("finished_at"):
                row["finished_at"] = int(time.time())
            row.setdefault("attempt", 0)
            target.execute(sql.SQL("INSERT INTO jobs ({}) VALUES ({})").format(
                sql.SQL(",").join(map(sql.Identifier, columns)),
                sql.SQL(",").join(sql.Placeholder() for _ in columns)),
                [row.get(column) for column in columns])
        for row in metrics:
            columns = list(row)
            target.execute(sql.SQL("INSERT INTO conversion_metrics ({}) VALUES ({})").format(
                sql.SQL(",").join(map(sql.Identifier, columns)),
                sql.SQL(",").join(sql.Placeholder() for _ in columns)), list(row.values()))
        if not args.apply:
            target.rollback()
    verb = "Importados" if args.apply else "Comprobados, sin guardar"
    print(f"{verb}: {len(jobs)} trabajos y {len(metrics)} acumulados. SQLite y sus archivos permanecen intactos.")


if __name__ == "__main__":
    main()
