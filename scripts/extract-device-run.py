#!/usr/bin/env python3
"""Pull the run record out of an xcodebuild log and write it as a committed run file.

    xcodebuild test ... 2>&1 | tee /tmp/device-run.log
    scripts/extract-device-run.py /tmp/device-run.log specs/007-ffi-surface/runs/

The record is printed by DeviceMeasurementTests between XTRIEVER_DEVICE_RUN_BEGIN / _END. The
file name is <device>-<date>-<loadPath>-threads<n>.json, derived from the record itself.
"""
import json
import pathlib
import sys

log, out_dir = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = log.read_text(errors="replace")
begin, end = "XTRIEVER_DEVICE_RUN_BEGIN", "XTRIEVER_DEVICE_RUN_END"
if begin not in text or end not in text:
    sys.exit(f"no run record in {log}: did DeviceMeasurementTests run (models + SciFact bundled)?")
body = text[text.index(begin) + len(begin):text.index(end)]
# Xcode prefixes console lines with a timestamp + process tag; strip anything before the first '{'.
body = body[body.index("{"):]
record = json.loads(body)
threads = record["build"].get("rayonNumThreads") or "default"
stamp = record["recordedAt"].replace(":", "").replace("-", "")[:15]
name = f"{record['device']}-{stamp}-{record['build']['loadPath']}-threads{threads}.json"
out_dir.mkdir(parents=True, exist_ok=True)
path = out_dir / name
path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
fp = record["footprint"]
print(f"wrote {path}")
print(f"  device {record['device']} {record['os']} thermal={record['thermalState']} {record['build']['configuration']} sim={record['build']['isSimulator']}")
print(f"  footprint peak {fp['peakBytes']:,} B ({fp['peakMethod']}) vs ceiling {fp['ceilingBytes']:,} → {fp['verdict']}")
if "perDepthMeanMs" in record:  # 007/008 harness records
    print(f"  per-depth mean ms {record['perDepthMeanMs']}  per-pair {record['derivedPerPairMs']}")
    p = record["parity"]
    print(f"  parity {p['verdict']} (lexical {p['lexicalBitIdentical']}/{p['queriesCompared']}, dense Δ {p['denseMaxAbsDiff']}, rerank Δ {p['rerankMaxAbsDiff']})")
if "latency" in record:  # 009 demo records
    print(f"  latency {record['latency']}")
