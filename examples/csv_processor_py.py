#!/usr/bin/env python3
"""CSV Processor Example - Python SDK

This example demonstrates how to:
1. Mount a file from the host into the guest VM using file_mounts
2. Run a Python script that processes the file inside Nanvix

NOTE: This example currently requires a Nanvix fix to work.
When using pre-built FAT images (for Python stdlib), file mounts
and ramfs are not currently supported.

Run with:
    maturin develop --features python
    python examples/csv_processor_py.py
"""

import asyncio
import os
import tempfile
from pathlib import Path

from hyperlight_nanvix import NanvixSandbox, SandboxConfig


async def main():
    """Run CSV processor example."""
    # Setup: Create sample CSV in a temp directory
    with tempfile.TemporaryDirectory() as temp_dir:
        temp_path = Path(temp_dir)

        # Create input CSV file
        csv_path = temp_path / "input.csv"
        csv_path.write_text(
            "name,age,city\n"
            "Alice,30,NYC\n"
            "Bob,25,LA\n"
            "Charlie,35,Chicago\n"
        )

        print(f"Created sample CSV at: {csv_path}")

        # Get nanvix registry path from environment or use default (dist/ folder)
        script_dir = Path(__file__).resolve().parent.parent
        nanvix_registry = os.environ.get("NANVIX_REGISTRY", str(script_dir / "dist"))

        # Configure sandbox with file mounts
        # Nanvix copies files to FAT automatically
        config = SandboxConfig(
            log_directory="/tmp/hyperlight-nanvix",
            tmp_directory="/tmp/hyperlight-nanvix",
            nanvix_registry=nanvix_registry,
            file_mounts=[
                (str(csv_path), "/tmp/test.csv")  # (host_path, guest_path)
            ]
        )

        sandbox = NanvixSandbox(config)

        print("Running CSV processor...")
        result = await sandbox.run("guest-examples/csv_processor.py")

        if result.success:
            # Output is printed directly to stdout by the guest
            print("Processing complete!")
        else:
            print(f"Error: {result.error}")
            return 1

    return 0


if __name__ == "__main__":
    exit(asyncio.run(main()))
