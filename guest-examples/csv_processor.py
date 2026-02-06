#!/usr/bin/env python3
"""CSV processor example - reads from mounted file, outputs JSON to stdout.

NOTE: This script requires a Nanvix fix to work properly.
Currently, file mounts don't work when using pre-built FAT images
(needed for Python stdlib). See hyperlight-nanvix README for details.
"""

import csv
import json
import sys


def process_csv(input_path):
    """Read CSV, process data, output JSON result to stdout."""
    try:
        # Read CSV file (mounted via file_mounts)
        with open(input_path, 'r') as csvfile:
            reader = csv.DictReader(csvfile)
            rows = list(reader)

        # Log to stderr
        print(f"Read {len(rows)} rows from {input_path}", file=sys.stderr)

        # Process data (example: count by category, calculate stats)
        result = {
            "success": True,
            "total_rows": len(rows),
            "columns": list(rows[0].keys()) if rows else [],
            "data": rows
        }

        # Output JSON to stdout
        print(json.dumps(result))
        return True

    except Exception as e:
        # Error output as JSON
        print(json.dumps({"success": False, "error": str(e)}))
        return False


if __name__ == "__main__":
    # Input: mounted file at /tmp/test.csv
    success = process_csv("/tmp/test.csv")
    sys.exit(0 if success else 1)
