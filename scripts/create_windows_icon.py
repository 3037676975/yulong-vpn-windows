from __future__ import annotations

from pathlib import Path


def validate_ico(path: Path) -> None:
    data = path.read_bytes()
    if len(data) < 100_000:
        raise RuntimeError(f"Windows icon is unexpectedly small: {len(data)} bytes")
    if data[:4] != b"\x00\x00\x01\x00":
        raise RuntimeError("Windows icon header is invalid")
    image_count = int.from_bytes(data[4:6], "little")
    if image_count < 6:
        raise RuntimeError(f"Windows icon has only {image_count} image sizes")


if __name__ == "__main__":
    icon = Path(__file__).resolve().parents[1] / "src-tauri" / "icons" / "icon.ico"
    validate_ico(icon)
    print(f"validated Android-matched Windows icon: {icon} ({icon.stat().st_size} bytes)")
