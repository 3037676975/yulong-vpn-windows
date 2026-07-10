from __future__ import annotations

import math
import struct
from pathlib import Path


def rgba_for_pixel(x: int, y: int, size: int) -> tuple[int, int, int, int]:
    cx = cy = (size - 1) / 2
    dx = (x - cx) / cx
    dy = (y - cy) / cy
    dist = math.sqrt(dx * dx + dy * dy)
    if dist > 0.94:
        return (0, 0, 0, 0)

    shade = max(0.0, 1.0 - dist)
    highlight = max(0.0, 1.0 - math.sqrt((dx + 0.35) ** 2 + (dy + 0.45) ** 2))

    r = int(28 + 70 * highlight + 10 * shade)
    g = int(150 + 90 * shade + 35 * highlight)
    b = int(115 + 65 * highlight + 20 * shade)
    a = 255

    band = abs((dy + 0.15) - 0.45 * math.sin(dx * 2.6))
    if band < 0.10 and -0.75 < dx < 0.75 and -0.55 < dy < 0.55:
        r = min(255, r + 70)
        g = min(255, g + 80)
        b = min(255, b + 55)

    if dist > 0.80:
        r = int(r * 0.78)
        g = int(g * 0.83)
        b = int(b * 0.80)

    return (r, g, b, a)


def make_dib(size: int) -> bytes:
    width = height = size
    pixels = bytearray()
    for y in range(height - 1, -1, -1):
        for x in range(width):
            r, g, b, a = rgba_for_pixel(x, y, size)
            pixels.extend((b, g, r, a))

    mask_row_bytes = ((width + 31) // 32) * 4
    and_mask = bytes(mask_row_bytes * height)

    header = struct.pack(
        '<IIIHHIIIIII',
        40,
        width,
        height * 2,
        1,
        32,
        0,
        len(pixels),
        0,
        0,
        0,
        0,
    )
    return header + bytes(pixels) + and_mask


def write_ico(path: Path, sizes: list[int]) -> None:
    images = [make_dib(size) for size in sizes]
    header_size = 6 + len(sizes) * 16
    offset = header_size
    entries = bytearray()
    for size, image in zip(sizes, images):
        width_byte = 0 if size >= 256 else size
        height_byte = 0 if size >= 256 else size
        entries.extend(struct.pack('<BBBBHHII', width_byte, height_byte, 0, 0, 1, 32, len(image), offset))
        offset += len(image)

    data = struct.pack('<HHH', 0, 1, len(sizes)) + bytes(entries) + b''.join(images)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


if __name__ == '__main__':
    target = Path(__file__).resolve().parents[1] / 'src-tauri' / 'icons' / 'icon.ico'
    write_ico(target, [16, 32, 48, 64, 128, 256])
    print(f'created {target}')
