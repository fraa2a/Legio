//! Markdown to HTML for package readmes and changelogs. The output is safe by
//! construction: raw HTML becomes escaped text, links keep only web addresses
//! and images are never loaded, they become links to the image.

use pulldown_cmark::{CowStr, Event, LinkType, Options, Parser, Tag, TagEnd, html};

use super::external_url::parse_external_url;

pub(crate) fn render_markdown(markdown: &str) -> String {
    let mut parser = Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
    )
    .peekable();
    let mut events: Vec<Event> = Vec::new();
    // One entry per open link or image: whether it was kept as an anchor.
    let mut open: Vec<bool> = Vec::new();
    let mut anchors = 0_usize;
    while let Some(event) = parser.next() {
        let link = match &event {
            Event::Start(Tag::Link { dest_url, .. }) => Some((dest_url.clone(), false)),
            Event::Start(Tag::Image { dest_url, .. }) => Some((dest_url.clone(), true)),
            _ => None,
        };
        if let Some((dest_url, is_image)) = link {
            // Anchors cannot nest, so a linked image keeps only the outer link.
            let target = if anchors == 0 {
                parse_external_url(&dest_url)
            } else {
                None
            };
            let Some(url) = target else {
                open.push(false);
                continue;
            };
            open.push(true);
            anchors += 1;
            events.push(Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: CowStr::from(url.to_string()),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }));
            // An image without alt text is shown as its address.
            if is_image && matches!(parser.peek(), Some(Event::End(TagEnd::Image))) {
                events.push(Event::Text(CowStr::from(url.to_string())));
            }
            continue;
        }
        match event {
            Event::Html(text) | Event::InlineHtml(text) => events.push(Event::Text(text)),
            Event::End(TagEnd::Link | TagEnd::Image) => {
                if open.pop() == Some(true) {
                    anchors -= 1;
                    events.push(Event::End(TagEnd::Link));
                }
            }
            other => events.push(other),
        }
    }
    let mut output = String::new();
    html::push_html(&mut output, events.into_iter());
    output
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
            "<code class=\"language-rust\">",
            "<table>",
            "<td>1</td>",
            "<del>gone</del>",
        ] {
            assert!(html.contains(expected), "{expected} in {html}");
        }
    }

    #[test]
    fn escapes_raw_html() {
        let html = render_markdown(
            "<script>alert(1)</script>\n\nText <b onclick=\"x()\">bold</b> <img src=x onerror=y>\n\n<div>\nblock\n</div>\n",
        );
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<b "), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(!html.contains("<div"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(html.contains("&lt;img src=x onerror=y&gt;"), "{html}");
    }

    #[test]
    fn keeps_only_web_links() {
        let html = render_markdown(
            "[ok](https://example.com/a) [bad](javascript:alert(1)) [file](file:///C:/x) [rel](/x) [anchor](#top) <mailto:a@example.com>\n",
        );
        assert!(
            html.contains("<a href=\"https://example.com/a\">ok</a>"),
            "{html}"
        );
        assert_eq!(html.matches("<a ").count(), 1, "{html}");
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
    fn turns_images_into_links() {
        let html = render_markdown(
            "![A logo](https://example.com/logo.png)\n\n![](https://example.com/plain.png)\n\n![bad](data:image/png;base64,AAAA)\n\n![local](logo.png)\n",
        );
        assert!(!html.contains("<img"), "{html}");
        assert!(
            html.contains("<a href=\"https://example.com/logo.png\">A logo</a>"),
            "{html}"
        );
        assert!(
            html.contains(
                "<a href=\"https://example.com/plain.png\">https://example.com/plain.png</a>"
            ),
            "{html}"
        );
        assert!(html.contains("bad"), "{html}");
        assert!(!html.contains("data:"), "{html}");
        assert!(html.contains("local"), "{html}");
        assert_eq!(html.matches("<a ").count(), 2, "{html}");
    }

    #[test]
    fn never_nests_anchors() {
        let html = render_markdown(
            "[![badge](https://img.example.com/b.svg)](https://example.com/page)\n",
        );
        assert_eq!(html.matches("<a ").count(), 1, "{html}");
        assert!(
            html.contains("<a href=\"https://example.com/page\">badge</a>"),
            "{html}"
        );
    }

    #[test]
    fn empty_input_renders_nothing() {
        assert_eq!(render_markdown(""), "");
    }
}
