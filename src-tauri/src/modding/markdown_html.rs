//! Markdown to sanitized HTML for package readmes and changelogs. Authors may
//! embed raw HTML, so the rendered document goes through an allowlist sanitizer:
//! scripts, styles and event handlers are dropped, and links and images keep
//! only plain web addresses.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::LazyLock;

use ammonia::{Builder, UrlRelative};
use pulldown_cmark::{Options, Parser, html};

use super::external_url::parse_external_url;

static SANITIZER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let mut builder = Builder::default();
    builder
        .url_schemes(HashSet::from(["http", "https"]))
        .url_relative(UrlRelative::Deny)
        .add_generic_attributes(&["align"])
        .attribute_filter(|_, attribute, value| match attribute {
            "href" | "src" => parse_external_url(value).map(|url| Cow::Owned(url.to_string())),
            _ => Some(Cow::Borrowed(value)),
        });
    builder
});

pub(crate) fn render_markdown(markdown: &str) -> String {
    let mut output = String::new();
    html::push_html(
        &mut output,
        Parser::new_ext(
            markdown,
            Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
        ),
    );
    SANITIZER.clean(&output).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basic_markdown() {
        let html = render_markdown(
            "# Title\n\nSome **bold** and `code`.\n\n- one\n- two\n\n```rust\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n~~gone~~\n",
        );
        for expected in [
            "<h1>Title</h1>",
            "<strong>bold</strong>",
            "<code>code</code>",
            "<li>one</li>",
            "<pre><code>",
            "<table>",
            "<td>1</td>",
            "<del>gone</del>",
        ] {
            assert!(html.contains(expected), "{expected} in {html}");
        }
    }

    #[test]
    fn renders_safe_raw_html() {
        let html = render_markdown(
            "Line one<br>Line two\n\n<b>bold</b> <p align=\"center\"><img src=\"https://example.com/a.png\" alt=\"A\"></p>\n",
        );
        for expected in [
            "Line one<br>Line two",
            "<b>bold</b>",
            "<p align=\"center\">",
            "<img src=\"https://example.com/a.png\" alt=\"A\">",
        ] {
            assert!(html.contains(expected), "{expected} in {html}");
        }
    }

    #[test]
    fn drops_unsafe_raw_html() {
        let html = render_markdown(
            "<script>alert(1)</script>\n\nText <b onclick=\"x()\">bold</b> <img src=x onerror=y>\n\n<div>\nblock\n</div>\n",
        );
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("alert(1)"), "{html}");
        assert!(!html.contains("onclick"), "{html}");
        assert!(!html.contains("onerror"), "{html}");
        assert!(!html.contains("src=\"x\""), "{html}");
        assert!(html.contains("<b>bold</b>"), "{html}");
        assert!(html.contains("<div>"), "{html}");
        assert!(html.contains("block"), "{html}");
    }

    #[test]
    fn keeps_only_web_links() {
        let html = render_markdown(
            "[ok](https://example.com/a) [bad](javascript:alert(1)) [file](file:///C:/x) [rel](/x) [anchor](#top) <mailto:a@example.com>\n",
        );
        assert!(
            html.contains("<a href=\"https://example.com/a\" rel=\"noopener noreferrer\">ok</a>"),
            "{html}"
        );
        assert_eq!(
            html.matches("href=\"https://example.com/a\"").count(),
            1,
            "{html}"
        );
        assert_eq!(html.matches("<a ").count(), 6, "{html}");
        assert!(
            html.contains("bad"),
            "the text of a dropped link stays: {html}"
        );
        assert!(!html.contains("javascript"), "{html}");
        assert!(!html.contains("file:"), "{html}");
    }

    #[test]
    fn escapes_quotes_in_link_addresses() {
        let html = render_markdown("[x](<https://example.com/\"onmouseover=\"alert(1)>)\n");
        assert!(!html.contains("\"onmouseover"), "{html}");
        assert_eq!(html.matches("<a ").count(), 1, "{html}");
    }

    #[test]
    fn renders_images() {
        let html = render_markdown(
            "![A logo](https://example.com/logo.png)\n\n![](https://example.com/plain.png)\n\n![bad](data:image/png;base64,AAAA)\n\n![local](logo.png)\n",
        );
        assert!(
            html.contains("<img src=\"https://example.com/logo.png\" alt=\"A logo\">"),
            "{html}"
        );
        assert!(
            html.contains("<img src=\"https://example.com/plain.png\" alt=\"\">"),
            "{html}"
        );
        assert!(!html.contains("data:"), "{html}");
        assert!(!html.contains("src=\"logo.png\""), "{html}");
        assert_eq!(
            html.matches("<img ").count(),
            4,
            "broken sources stay as images without an address: {html}"
        );
    }

    #[test]
    fn keeps_a_linked_image_inside_its_anchor() {
        let html = render_markdown(
            "[![badge](https://img.example.com/b.svg)](https://example.com/page)\n",
        );
        assert_eq!(html.matches("<a ").count(), 1, "{html}");
        assert_eq!(
            html.matches("href=\"https://example.com/page\"").count(),
            1,
            "{html}"
        );
        assert!(
            html.contains("<img src=\"https://img.example.com/b.svg\" alt=\"badge\">"),
            "{html}"
        );
    }

    #[test]
    fn empty_input_renders_nothing() {
        assert_eq!(render_markdown(""), "");
    }
}
