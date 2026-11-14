"""Check the local Markdown/HTML link conventions used by the bundled guides."""
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit


def verify_local_links(files, boundary):
    boundary = Path(boundary).resolve()
    checked = 0
    for path in files:
        path = Path(path)
        text = path.read_text(encoding='utf-8')
        for pair in re.findall(r'(?:src|href)="([^"]+)"|\]\(([^)]+)\)', text):
            link = next(value for value in pair if value).strip().strip('<>')
            parsed = urlsplit(link)
            if parsed.scheme or parsed.netloc or not parsed.path:
                continue
            target = (path.parent / unquote(parsed.path)).resolve()
            if not target.is_relative_to(boundary) or not target.exists():
                raise ValueError(f'Broken bundled link in {path}: {link}')
            checked += 1
    return checked
