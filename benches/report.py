"""Prints criterion's frame benchmarks as a Markdown table, for CI's job summary.

Run after `cargo bench -p cg_bench --bench frames`:

    python benches/report.py target/criterion
"""
import json
import sys
from pathlib import Path


def main() -> None:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "target/criterion")
    rows = []
    for estimates in sorted(root.glob("frame/*/new/estimates.json")):
        bench = json.loads((estimates.parent / "benchmark.json").read_text(encoding="utf-8"))
        mean = json.loads(estimates.read_text(encoding="utf-8"))["mean"]
        rows.append(
            (
                bench.get("value_str") or bench["function_id"],
                mean["point_estimate"],
                mean["confidence_interval"]["lower_bound"],
                mean["confidence_interval"]["upper_bound"],
            )
        )
    print("### Frame benchmarks")
    print()
    print("One frame of each scene without a window (`cargo bench -p cg_bench`). Shared")
    print("runners are noisy: compare runs on the same machine before trusting a change.")
    print()
    print("| Scene | Mean | 95% interval | Frames a second |")
    print("| --- | ---: | ---: | ---: |")
    for name, mean, low, high in rows:
        print(f"| {name} | {mean / 1e6:.3f} ms | {low / 1e6:.3f} to {high / 1e6:.3f} ms | {1e9 / mean:,.0f} |")
    if not rows:
        print("| (no results found) | | | |")


if __name__ == "__main__":
    main()
