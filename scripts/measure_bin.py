#!/usr/bin/env python3
import sys
import subprocess
import time
import os

if len(sys.argv) < 2:
    print("Usage: measure_bin.py <cmd...>")
    sys.exit(1)

cmd = sys.argv[1:]
out_log = "/tmp/bench_out.log"

start = time.perf_counter()
with open(out_log, "wb") as f:
    proc = subprocess.Popen(cmd, stdout=f, stderr=f)
    pid = proc.pid
    _, exit_status, rusage = os.wait4(pid, 0)
elapsed_ms = (time.perf_counter() - start) * 1000.0

# rusage.ru_maxrss is in KB on Linux
peak_rss_mb = rusage.ru_maxrss / 1024.0

print(f"BENCH_WALL_MS={elapsed_ms:.2f}")
print(f"BENCH_PEAK_RSS_MB={peak_rss_mb:.2f}")

if os.path.exists(out_log):
    with open(out_log, "r", errors="replace") as f:
        for line in f:
            if "BENCHMARK_RESULT_MS=" in line:
                print(line.strip())
