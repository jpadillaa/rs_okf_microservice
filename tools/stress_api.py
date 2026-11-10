#!/usr/bin/env python3
"""Prueba acotada de carga de la API; los originales y ZIP quedan solo en --output.

python3 tools/stress_api.py datos_de_prueba/documento.pdf --requests 3 --concurrency 2 \
    --url http://127.0.0.1:8080 --output output/carga

Mide subida, espera hasta observar processing, tiempo hasta terminar, descarga y
respuesta de /health bajo carga. La espera observada tiene la resolución del sondeo;
la duración exacta de conversión y su RSS se consultan en el registro del worker.
Cada llamada crea trabajos reales: usar una instancia de pruebas con su propio almacén.
"""

import argparse
import concurrent.futures
import contextlib
import http.client
import json
import shutil
import statistics
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path


def get(url, timeout=30):
    start = time.monotonic()
    with urllib.request.urlopen(url, timeout=timeout) as response:
        return json.load(response), time.monotonic() - start


def upload(url, original):
    boundary = "okf-" + uuid.uuid4().hex
    # Nombre ASCII fijo: no altera los bytes del original ni requiere escapar nombres
    # arbitrarios en Content-Disposition.
    prefix = (
        f'--{boundary}\r\nContent-Disposition: form-data; name="file"; '
        f'filename="stress{original.suffix.lower()}"\r\n'
        'Content-Type: application/octet-stream\r\n\r\n'
    ).encode()
    suffix = f"\r\n--{boundary}--\r\n".encode()
    parsed = urllib.parse.urlsplit(url)
    cls = http.client.HTTPSConnection if parsed.scheme == "https" else http.client.HTTPConnection
    start = time.monotonic()
    with contextlib.closing(cls(parsed.hostname, parsed.port, timeout=120)) as connection:
        connection.putrequest("POST", parsed.path.rstrip("/") + "/jobs")
        connection.putheader("Content-Type", f"multipart/form-data; boundary={boundary}")
        connection.putheader("Content-Length", str(len(prefix) + original.stat().st_size + len(suffix)))
        connection.endheaders()
        connection.send(prefix)
        with original.open("rb") as stream:
            while chunk := stream.read(1024 * 1024):
                connection.send(chunk)
        connection.send(suffix)
        response = connection.getresponse()
        payload = response.read()
        elapsed = time.monotonic() - start
        if response.status != 202:
            return {"http_status": response.status, "upload_s": elapsed}
        return {"http_status": 202, "upload_s": elapsed, **json.loads(payload)}


def run_job(index, args):
    start = time.monotonic()
    result = {"request": index}
    try:
        result.update(upload(args.url, args.original))
        if result["http_status"] != 202:
            return result
        deadline = time.monotonic() + args.timeout
        while time.monotonic() < deadline:
            state, _ = get(f"{args.url}/jobs/{result['job_id']}")
            result.update({k: v for k, v in state.items() if k != "error"})
            if state["status"] == "processing" and "processing_observed_s" not in result:
                result["processing_observed_s"] = time.monotonic() - start
            if state["status"] in ("complete", "partial", "failed", "expired"):
                result["finished_s"] = time.monotonic() - start
                result["error_tail"] = (state.get("error") or "")[-1000:]
                break
            time.sleep(args.poll)
        else:
            result["client_timeout"] = True
        if result.get("status") in ("complete", "partial"):
            target = args.output / f"bundle-{index:03}.zip"
            download = time.monotonic()
            with urllib.request.urlopen(f"{args.url}/jobs/{result['job_id']}/bundle", timeout=120) as response:
                with target.open("wb") as stream:
                    shutil.copyfileobj(response, stream, 1024 * 1024)
            result["download_s"] = time.monotonic() - download
            result["bundle_bytes"] = target.stat().st_size
            result["bundle"] = str(target)
    except Exception as error:
        result["client_error"] = str(error)[:1000]
    result["total_s"] = time.monotonic() - start
    return result


def probe(args, stop, results):
    while not stop.is_set():
        start = time.monotonic()
        sample = {"timestamp_unix_s": time.time(), "monotonic_s": start}
        try:
            state, elapsed = get(args.url + "/health", timeout=5)
            sample.update({"elapsed_s": elapsed, "ok": state.get("status") == "ok", "http_status": 200})
            sample.update({k: state[k] for k in ("database", "storage", "worker", "worker_age_s") if k in state})
        except urllib.error.HTTPError as error:
            sample.update({"elapsed_s": time.monotonic() - start, "ok": False, "http_status": error.code})
            try:
                state = json.load(error)
                sample.update({k: state[k] for k in ("database", "storage", "worker", "worker_age_s") if k in state})
            except (ValueError, OSError):
                pass
        except Exception as error:
            sample.update({"elapsed_s": time.monotonic() - start, "ok": False, "error_type": type(error).__name__})
        results.append(sample)
        stop.wait(args.poll)


def snapshot_metrics(args, name):
    with urllib.request.urlopen(args.url + "/metrics", timeout=30) as response:
        (args.output / name).write_bytes(response.read())


def positive(value):
    number = float(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("debe ser mayor que cero")
    return number


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("--url", default="http://127.0.0.1:8080")
    parser.add_argument("--requests", type=int, default=1)
    parser.add_argument("--concurrency", type=int, default=1)
    parser.add_argument("--timeout", type=positive, default=900)
    parser.add_argument("--poll", type=positive, default=0.5)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.requests < 1 or args.concurrency < 1:
        parser.error("requests y concurrency deben ser mayores que cero")
    parsed = urllib.parse.urlsplit(args.url)
    if parsed.scheme not in ("http", "https") or not parsed.hostname or parsed.query or parsed.fragment:
        parser.error("url debe ser una base HTTP(S) sin consulta ni fragmento")
    if not args.original.is_file():
        parser.error("el original no existe")
    args.url = args.url.rstrip("/")
    args.output.mkdir(parents=True, exist_ok=False)
    snapshot_metrics(args, "metrics-before.txt")
    health = []
    stop = threading.Event()
    thread = threading.Thread(target=probe, args=(args, stop, health), daemon=True)
    thread.start()
    started = time.monotonic()
    wall_started = time.time()
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
            jobs = list(pool.map(lambda i: run_job(i, args), range(1, args.requests + 1)))
    finally:
        stop.set()
        thread.join()
    elapsed = time.monotonic() - started
    wall_elapsed = time.time() - wall_started
    (args.output / "health-samples.json").write_text(json.dumps(health, indent=2) + "\n")
    snapshot_metrics(args, "metrics-after.txt")
    latencies = sorted(x["elapsed_s"] for x in health)
    successful = sum(x.get("status") in ("complete", "partial") for x in jobs)
    report = {
        "requests": args.requests,
        "concurrency": args.concurrency,
        "poll_s": args.poll,
        "elapsed_s": elapsed,
        "wall_elapsed_s": wall_elapsed,
        "wall_monotonic_difference_s": wall_elapsed - elapsed,
        "successful": successful,
        "throughput_per_minute": successful * 60 / elapsed,
        "health_samples": len(health),
        "health_failures": sum(not x["ok"] for x in health),
        "health_median_s": statistics.median(latencies) if latencies else None,
        "health_p95_s": latencies[min(len(latencies) - 1, int(len(latencies) * 0.95))] if latencies else None,
        "jobs": jobs,
    }
    (args.output / "results.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "jobs"}, ensure_ascii=False))
    if successful != args.requests or report["health_failures"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
