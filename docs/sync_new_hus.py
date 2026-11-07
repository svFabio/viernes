"""Syncs the new Rust HUs (HU-RUST-21 to HU-RUST-28) to GitHub Issues and GitHub Project 12."""

import subprocess
import time
from pathlib import Path
import sys

# Add docs to sys.path to import HUS
sys.path.insert(0, str(Path("/home/svfabio/Projects/09-viernes/docs")))
from generate_rust_hus import HUS

REPO = "svFabio/viernes"
PROJECT_NUM = "12"
OWNER = "svFabio"
HUS_DIR = Path("/home/svfabio/Projects/09-viernes/docs/hus-rust")


def run_gh(cmd: list[str]) -> str:
    res = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return res.stdout.strip()


def main():
    new_hus = [h for h in HUS if int(h["id"].split("-")[-1]) >= 21]
    print(f"Syncing {len(new_hus)} new HUs to {REPO} and Project {PROJECT_NUM}...")

    total_created = 0
    for idx, hu in enumerate(new_hus, 21):
        hu_id = hu["id"]
        hu_title = f"[{hu_id}] {hu['title']}"
        labels = f"HU,rust,{hu['epic']},{hu['risk']}"

        matches = list(HUS_DIR.glob(f"{hu_id}-*.md"))
        if not matches:
            print(f"Warning: No markdown file found for {hu_id}")
            continue
        body = matches[0].read_text(encoding="utf-8")

        print(f"\n[{idx}/28] Creating HU Issue: {hu_title}...")
        try:
            hu_url = run_gh([
                "gh", "issue", "create",
                "--repo", REPO,
                "--title", hu_title,
                "--body", body,
                "--label", labels,
            ])
            print(f"  ✓ Created: {hu_url}")
            total_created += 1

            run_gh([
                "gh", "project", "item-add", PROJECT_NUM,
                "--owner", OWNER,
                "--url", hu_url,
            ])
            print(f"  ✓ Added to Project {PROJECT_NUM}")
            time.sleep(0.3)

            for st_id, st_name in hu["subtasks"]:
                st_title = f"[{st_id}] {st_name}"
                st_body = (
                    f"Subtarea técnica perteneciente a {hu_title}.\n\n"
                    f"Criterios y especificación detallada en la Historia de Usuario padre: {hu_url}.\n"
                    f"Asignado a: {hu['agent']}."
                )
                st_labels = f"subtask,rust,{hu['epic']},{hu['risk']}"

                st_url = run_gh([
                    "gh", "issue", "create",
                    "--repo", REPO,
                    "--title", st_title,
                    "--body", st_body,
                    "--label", st_labels,
                ])
                print(f"    - Subtask {st_id}: {st_url}")
                total_created += 1

                run_gh([
                    "gh", "project", "item-add", PROJECT_NUM,
                    "--owner", OWNER,
                    "--url", st_url,
                ])
                time.sleep(0.3)

        except subprocess.CalledProcessError as exc:
            print(f"Error creating issue for {hu_id}: {exc.stderr}")

    print(f"\nDone! Successfully synchronized {total_created} items into GitHub Project {PROJECT_NUM}.")


if __name__ == "__main__":
    main()
