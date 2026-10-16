"""
markdown.py — Markdown parsing helper using markdown-it-py.
Extracts heading slugs, explicit HTML anchors, inline/reference links, and images with file:line locations.
"""

import re
import urllib.parse
from markdown_it import MarkdownIt


class ParsedLink:
    def __init__(self, source_file, line_no, raw_url, link_type="inline", text="", is_image=False):
        self.source_file = source_file
        self.line_no = line_no
        self.raw_url = raw_url
        self.link_type = link_type
        self.text = text
        self.is_image = is_image

    def __repr__(self):
        return f"<ParsedLink {self.source_file}:{self.line_no} -> {self.raw_url}>"


def slugify_heading(heading_text):
    """
    GitHub-style heading slug generator.
    1. Strip HTML tags
    2. Lowercase
    3. Strip punctuation except unicode word characters, spaces, and hyphens
    4. Replace whitespace with hyphens
    """
    # Strip HTML tags
    text = re.sub(r"<[^>]+>", "", heading_text)
    # Lowercase
    text = text.lower()
    # Strip punctuation (keep unicode word characters \w, whitespace \s, and hyphens -)
    text = re.sub(r"[^\w\s-]", "", text, flags=re.UNICODE)
    # Replace spaces / whitespace with hyphens
    text = re.sub(r"\s+", "-", text)
    return text


class MarkdownDocument:
    def __init__(self, filepath, content):
        self.filepath = filepath
        self.content = content
        self.md_parser = MarkdownIt("commonmark").enable("table")
        self.tokens = self.md_parser.parse(content)
        self.anchors = set()
        self.slug_counts = {}
        self.links = []
        self.diagnostics = []

        self._parse()

    def _parse(self):
        tokens = self.tokens
        i = 0
        n = len(tokens)

        while i < n:
            t = tokens[i]
            # Ignore code blocks / fences so code block content is never treated as links
            if t.type in ("fence", "code_block"):
                i += 1
                continue

            # Extract headings
            if t.type == "heading_open":
                line_no = t.map[0] + 1 if t.map else 1
                if i + 1 < n and tokens[i + 1].type == "inline":
                    inline_tok = tokens[i + 1]
                    heading_text = self._get_inline_plaintext(inline_tok)
                    raw_slug = slugify_heading(heading_text)

                    if raw_slug in self.slug_counts:
                        self.slug_counts[raw_slug] += 1
                        unique_slug = f"{raw_slug}-{self.slug_counts[raw_slug]}"
                    else:
                        self.slug_counts[raw_slug] = 0
                        unique_slug = raw_slug

                    self.anchors.add(unique_slug)

            # Extract explicit HTML anchors (<a name="...">, <a id="...">, <element id="...">)
            if t.type in ("html_block", "inline"):
                html_text = t.content
                if t.children:
                    for c in t.children:
                        if c.type == "html_inline":
                            html_text += " " + c.content

                for m in re.finditer(r'''(?:name|id)\s*=\s*["']([^"']+)["']''', html_text, re.IGNORECASE):
                    self.anchors.add(m.group(1))

            # Extract links and images from inline tokens
            if t.type == "inline" and t.children:
                line_no = t.map[0] + 1 if t.map else 1
                self._extract_inline_links(t.children, line_no)

            i += 1

    def _get_inline_plaintext(self, inline_token):
        text = ""
        if not inline_token.children:
            return inline_token.content
        for c in inline_token.children:
            if c.type in ("text", "code_inline"):
                text += c.content
            elif c.type == "image":
                text += c.content or getattr(c, "attrs", {}).get("alt", "")
        return text

    def _extract_inline_links(self, children, base_line_no):
        i = 0
        while i < len(children):
            c = children[i]
            attrs = c.attrs if hasattr(c, "attrs") and c.attrs else {}

            if c.type == "link_open":
                url = attrs.get("href")
                if url:
                    link_text = ""
                    # Gather text inside link
                    j = i + 1
                    while j < len(children) and children[j].type != "link_close":
                        if children[j].type in ("text", "code_inline"):
                            link_text += children[j].content
                        j += 1
                    self.links.append(
                        ParsedLink(
                            source_file=self.filepath,
                            line_no=base_line_no,
                            raw_url=url,
                            link_type="inline",
                            text=link_text,
                            is_image=False,
                        )
                    )
            elif c.type == "image":
                url = attrs.get("src")
                alt = attrs.get("alt", "") or c.content
                if url:
                    self.links.append(
                        ParsedLink(
                            source_file=self.filepath,
                            line_no=base_line_no,
                            raw_url=url,
                            link_type="image",
                            text=alt,
                            is_image=True,
                        )
                    )

            i += 1
