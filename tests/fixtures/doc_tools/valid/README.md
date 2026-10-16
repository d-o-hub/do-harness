# Main Heading\r
\r
## Section One\r
First occurrence of section one.\r
\r
## Section One\r
Second occurrence of section one.\r
\r
## Heading with `code` & *style*!\r
Formatting heading.\r
\r
## Unicode 日本語\r
Unicode heading.\r
\r
<a name="custom-anchor"></a>\r
\r
Link to guide: [Guide](./docs/guide.md#section-one)\r
Link with space: [Space File](./docs/file%20with%20spaces.md#space-heading)\r
Reference link: [Ref Link][myref]\r
Image link: ![Alt Text](./docs/image.png)\r
GitHub URL: [GitHub Link](https://github.com/d-o-hub/do-harness/blob/main/docs/guide.md#section-one)\r
\r
[myref]: ./docs/guide.md\r
\r
```sh\r
# Code fence paths should be ignored\r
ls -la ./docs/nonexistent_file.md\r
```\r
