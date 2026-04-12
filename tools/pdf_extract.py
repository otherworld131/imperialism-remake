#!/usr/bin/env python3
"""Extract text from the Imperialism 1 manual PDF.

Usage:
    python3 tools/pdf_extract.py 17              # single page
    python3 tools/pdf_extract.py 15-20           # page range
    python3 tools/pdf_extract.py --search anarchy # search all pages for keyword
    python3 tools/pdf_extract.py --search "capital city" 10-30  # search within range
"""
import sys
import fitz

PDF_PATH = "docs/imperialism-1-manual.pdf"


def extract_pages(doc, start, end):
    for i in range(start, min(end + 1, len(doc))):
        text = doc[i].get_text()
        if text.strip():
            print(f"--- Page {i} ---")
            print(text)
            print()


def search_pages(doc, keyword, start=0, end=None):
    if end is None:
        end = len(doc) - 1
    keyword_lower = keyword.lower()
    for i in range(start, min(end + 1, len(doc))):
        text = doc[i].get_text()
        if keyword_lower in text.lower():
            print(f"--- Page {i} (match: {keyword}) ---")
            print(text)
            print()


def main():
    doc = fitz.open(PDF_PATH)
    args = sys.argv[1:]

    if not args:
        print(__doc__)
        sys.exit(1)

    keyword = None
    page_range = None

    i = 0
    while i < len(args):
        if args[i] == "--search":
            keyword = args[i + 1]
            i += 2
        else:
            page_range = args[i]
            i += 1

    start, end = 0, len(doc) - 1
    if page_range:
        if "-" in page_range:
            parts = page_range.split("-")
            start, end = int(parts[0]), int(parts[1])
        else:
            start = end = int(page_range)

    if keyword:
        search_pages(doc, keyword, start, end)
    else:
        extract_pages(doc, start, end)


if __name__ == "__main__":
    main()
