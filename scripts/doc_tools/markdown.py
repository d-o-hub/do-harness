"""
markdown.py — Markdown parsing helper using markdown-it-py.
Extracts heading slugs, explicit HTML anchors, inline/reference links, and images with file:line locations.
"""

import re

from markdown_it import MarkdownIt

from doc_tools.github_slugger import GithubSlugger, slugify


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
    """GitHub-style heading slug (github-slugger parity; see doc_tools/github_slugger.py)."""
    return slugify(heading_text)


# Anchor extraction scans real HTML tags only. Comments are stripped first, and
# the attribute lookbehind rejects suffix matches (`data-id="x"`), so prose,
# inline code, and fenced examples can never contribute anchors.
_HTML_COMMENT_RE = re.compile(r"<!--.*?-->", re.DOTALL)
_HTML_TAG_RE = re.compile(r"<[a-zA-Z][^>]*>", re.DOTALL)
_ANCHOR_ATTR_RE = re.compile(r"""(?<![\w-])(?:name|id)\s*=\s*["']([^"']+)["']""", re.IGNORECASE)


def _html_anchors(html_text):
    """Yield explicit anchor names (`<a name>`, any element `id`) from raw HTML."""
    for tag in _HTML_TAG_RE.finditer(_HTML_COMMENT_RE.sub("", html_text)):
        for match in _ANCHOR_ATTR_RE.finditer(tag.group(0)):
            yield match.group(1)


class MarkdownDocument:
    def __init__(self, filepath, content):
        self.filepath = filepath
        self.content = content
        self.md_parser = MarkdownIt("commonmark").enable("table")
        self.tokens = self.md_parser.parse(content)
        self.anchors = set()
        self.slugger = GithubSlugger()
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

            # Extract headings; duplicate slugs gain github-slugger's suffixes.
            if t.type == "heading_open":
                if i + 1 < n and tokens[i + 1].type == "inline":
                    inline_tok = tokens[i + 1]
                    heading_text = self._get_inline_plaintext(inline_tok)
                    self.anchors.add(self.slugger.slug(heading_text))

            # Extract explicit HTML anchors (<a name="...">, <a id="...">,
            # <element id="...">). Only real tags count: the raw inline token
            # source (`t.content`) would also match `id="x"` inside prose,
            # inline code, and link targets that are not anchors.
            if t.type == "html_block":
                self.anchors.update(_html_anchors(t.content))
            elif t.type == "inline" and t.children:
                for c in t.children:
                    if c.type == "html_inline":
                        self.anchors.update(_html_anchors(c.content))

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
