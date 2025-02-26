use html5ever::parse_document;
use html5ever::rcdom::{Handle, NodeData, RcDom};
use html5ever::serialize::{serialize, SerializeOpts};
use html5ever::tendril::TendrilSink;
use http::{is_valid_url, resolve_url, retrieve_asset};
use regex::Regex;
use std::default::Default;
use std::io;
use utils::data_to_dataurl;

lazy_static! {
    static ref EMPTY_STRING: String = String::new();
    static ref HAS_PROTOCOL: Regex = Regex::new(r"^[a-z0-9]+:").unwrap();
    static ref ICON_VALUES: Regex = Regex::new(
        r"^icon|shortcut icon|mask-icon|apple-touch-icon|fluid-icon$"
    ).unwrap();
}

const TRANSPARENT_PIXEL: &str = "data:image/png;base64,\
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";

const JS_DOM_EVENT_ATTRS: [&str; 21] = [
    // Input
    "onfocus",
    "onblur",
    "onselect",
    "onchange",
    "onsubmit",
    "onreset",
    "onkeydown",
    "onkeypress",
    "onkeyup",
    // Mouse
    "onmouseover",
    "onmouseout",
    "onmousedown",
    "onmouseup",
    "onmousemove",
    // Click
    "onclick",
    "ondblclick",
    // Load
    "onload",
    "onunload",
    "onabort",
    "onerror",
    "onresize",
];

fn get_parent_node_name(node: &Handle) -> String {
    let parent = node.parent.take().clone();
    let parent_node = parent.and_then(|node| node.upgrade()).unwrap();

    match &parent_node.data {
        NodeData::Document => { EMPTY_STRING.clone() }
        NodeData::Doctype { .. } => { EMPTY_STRING.clone() }
        NodeData::Text { .. } => { EMPTY_STRING.clone() }
        NodeData::Comment { .. } => { EMPTY_STRING.clone() }
        NodeData::Element { ref name, attrs: _, .. } => {
            name.local.as_ref().to_string()
        }
        NodeData::ProcessingInstruction { .. } => unreachable!()
    }
}

pub fn walk_and_embed_assets(
    url: &str,
    node: &Handle,
    opt_no_js: bool,
    opt_no_images: bool,
    opt_user_agent: &str,
    opt_silent: bool,
    opt_insecure: bool,
) {
    match node.data {
        NodeData::Document => {
            // Dig deeper
            for child in node.children.borrow().iter() {
                walk_and_embed_assets(
                        &url, child,
                        opt_no_js,
                        opt_no_images,
                        opt_user_agent,
                        opt_silent,
                        opt_insecure,
                    );
            }
        }
        NodeData::Doctype { .. } => {}
        NodeData::Text { .. } => {}
        NodeData::Comment { .. } => {
            // Note: in case of opt_no_js being set to true, there's no need to worry about
            //       getting rid of comments that may contain scripts, e.g. <!--[if IE]><script>...
            //       since that's not part of W3C standard and therefore gets ignored
            //       by browsers other than IE [5, 9]
        }
        NodeData::Element {
            ref name,
            ref attrs,
            ..
        } => {
            let attrs_mut = &mut attrs.borrow_mut();

            match name.local.as_ref() {
                "link" => {
                    let mut link_type = "";

                    for attr in attrs_mut.iter_mut() {
                        if &attr.name.local == "rel" {
                            if is_icon(&attr.value.to_string()) {
                                link_type = "icon";
                                break;
                            } else if attr.value.to_string() == "stylesheet" {
                                link_type = "stylesheet";
                                break;
                            }
                        }
                    }

                    if link_type == "icon" {
                        for attr in attrs_mut.iter_mut() {
                            if &attr.name.local == "href" {
                                if opt_no_images {
                                    attr.value.clear();
                                    attr.value.push_slice(TRANSPARENT_PIXEL);
                                } else {
                                    let href_full_url: String = resolve_url(
                                            &url,
                                            &attr.value.to_string()
                                        )
                                        .unwrap_or(EMPTY_STRING.clone());
                                    let favicon_datauri = retrieve_asset(
                                            &href_full_url,
                                            true,
                                            "",
                                            opt_user_agent,
                                            opt_silent,
                                            opt_insecure,
                                        )
                                        .unwrap_or(EMPTY_STRING.clone());
                                    attr.value.clear();
                                    attr.value.push_slice(favicon_datauri.as_str());
                                }
                            }
                        }
                    } else if link_type == "stylesheet" {
                        for attr in attrs_mut.iter_mut() {
                            if &attr.name.local == "href" {
                                let href_full_url: String = resolve_url(
                                        &url,
                                        &attr.value.to_string(),
                                    )
                                    .unwrap_or(EMPTY_STRING.clone());
                                let css_datauri = retrieve_asset(
                                        &href_full_url,
                                        true,
                                        "text/css",
                                        opt_user_agent,
                                        opt_silent,
                                        opt_insecure,
                                    )
                                    .unwrap_or(EMPTY_STRING.clone());
                                attr.value.clear();
                                attr.value.push_slice(css_datauri.as_str());
                            }
                        }
                    } else {
                        for attr in attrs_mut.iter_mut() {
                            if &attr.name.local == "href" {
                                let href_full_url: String = resolve_url(
                                        &url,
                                        &attr.value.to_string(),
                                    )
                                    .unwrap_or(EMPTY_STRING.clone());
                                attr.value.clear();
                                attr.value.push_slice(&href_full_url.as_str());
                            }
                        }
                    }
                }
                "img" => {
                    for attr in attrs_mut.iter_mut() {
                        if &attr.name.local == "src" {
                            let value = attr.value.to_string();

                            // Ignore images with empty source (they're hopelessly broken)
                            if value == EMPTY_STRING.clone() {
                                continue;
                            }

                            if opt_no_images {
                                attr.value.clear();
                                attr.value.push_slice(TRANSPARENT_PIXEL);
                            } else {
                                let src_full_url: String = resolve_url(
                                        &url,
                                        &value,
                                    )
                                    .unwrap_or(EMPTY_STRING.clone());
                                let img_datauri = retrieve_asset(
                                        &src_full_url,
                                        true,
                                        "",
                                        opt_user_agent,
                                        opt_silent,
                                        opt_insecure,
                                    )
                                    .unwrap_or(EMPTY_STRING.clone());
                                attr.value.clear();
                                attr.value.push_slice(img_datauri.as_str());
                            }
                        }
                    }
                }
                "source" => {
                    for attr in attrs_mut.iter_mut() {
                        if &attr.name.local == "srcset" {
                            if get_parent_node_name(&node) == "picture" {
                                if opt_no_images {
                                    attr.value.clear();
                                    attr.value.push_slice(TRANSPARENT_PIXEL);
                                } else {
                                    let srcset_full_url: String = resolve_url(
                                            &url,
                                            &attr.value.to_string(),
                                        )
                                        .unwrap_or(EMPTY_STRING.clone());
                                    let source_datauri = retrieve_asset(
                                            &srcset_full_url,
                                            true,
                                            "",
                                            opt_user_agent,
                                            opt_silent,
                                            opt_insecure,
                                        )
                                        .unwrap_or(EMPTY_STRING.clone());
                                    attr.value.clear();
                                    attr.value.push_slice(source_datauri.as_str());
                                }
                            }
                        }
                    }
                }
                "a" => {
                    for attr in attrs_mut.iter_mut() {
                        if &attr.name.local == "href" {
                            // Don't touch email links or hrefs which begin with a hash sign
                            if attr.value.starts_with('#') || has_protocol(&attr.value) {
                                continue;
                            }

                            let href_full_url: String = resolve_url(&url, &attr.value.to_string())
                                .unwrap_or(EMPTY_STRING.clone());
                            attr.value.clear();
                            attr.value.push_slice(href_full_url.as_str());
                        }
                    }
                }
                "script" => {
                    if opt_no_js {
                        // Get rid of src and inner content of SCRIPT tags
                        for attr in attrs_mut.iter_mut() {
                            if &attr.name.local == "src" {
                                attr.value.clear();
                            }
                        }
                        node.children.borrow_mut().clear();
                    } else {
                        for attr in attrs_mut.iter_mut() {
                            if &attr.name.local == "src" {
                                let src_full_url: String = resolve_url(
                                        &url,
                                        &attr.value.to_string(),
                                    )
                                    .unwrap_or(EMPTY_STRING.clone());
                                let js_datauri = retrieve_asset(
                                        &src_full_url,
                                        true,
                                        "application/javascript",
                                        opt_user_agent,
                                        opt_silent,
                                        opt_insecure,
                                    )
                                    .unwrap_or(EMPTY_STRING.clone());
                                attr.value.clear();
                                attr.value.push_slice(js_datauri.as_str());
                            }
                        }
                    }
                }
                "form" => {
                    for attr in attrs_mut.iter_mut() {
                        if &attr.name.local == "action" {
                            // Don't modify action that's already a full URL
                            if is_valid_url(&attr.value) {
                                continue;
                            }

                            let href_full_url: String = resolve_url(&url, &attr.value.to_string())
                                .unwrap_or(EMPTY_STRING.clone());
                            attr.value.clear();
                            attr.value.push_slice(href_full_url.as_str());
                        }
                    }
                }
                "iframe" => {
                    for attr in attrs_mut.iter_mut() {
                        if &attr.name.local == "src" {
                            let value = attr.value.to_string();

                            // Ignore iframes with empty source (they cause infinite loops)
                            if value == EMPTY_STRING.clone() {
                                continue;
                            }

                            let src_full_url: String = resolve_url(&url, &value)
                                .unwrap_or(EMPTY_STRING.clone());
                            let iframe_data = retrieve_asset(
                                    &src_full_url,
                                    false,
                                    "text/html",
                                    opt_user_agent,
                                    opt_silent,
                                    opt_insecure,
                                )
                                .unwrap_or(EMPTY_STRING.clone());
                            let dom = html_to_dom(&iframe_data);
                            walk_and_embed_assets(
                                    &src_full_url,
                                    &dom.document,
                                    opt_no_js,
                                    opt_no_images,
                                    opt_user_agent,
                                    opt_silent,
                                    opt_insecure,
                                );
                            let mut buf: Vec<u8> = Vec::new();
                            serialize(&mut buf, &dom.document, SerializeOpts::default()).unwrap();
                            let iframe_datauri = data_to_dataurl("text/html", &buf);
                            attr.value.clear();
                            attr.value.push_slice(iframe_datauri.as_str());
                        }
                    }
                }
                _ => {}
            }

            if opt_no_js {
                // Get rid of JS event attributes
                for attr in attrs_mut.iter_mut() {
                    if JS_DOM_EVENT_ATTRS.contains(&attr.name.local.to_lowercase().as_str()) {
                        attr.value.clear();
                    }
                }
            }

            // Dig deeper
            for child in node.children.borrow().iter() {
                walk_and_embed_assets(
                        &url,
                        child,
                        opt_no_js,
                        opt_no_images,
                        opt_user_agent,
                        opt_silent,
                        opt_insecure,
                    );
            }
        }
        NodeData::ProcessingInstruction { .. } => unreachable!()
    }
}

fn has_protocol(url: &str) -> bool {
    HAS_PROTOCOL.is_match(&url.to_lowercase())
}

pub fn html_to_dom(data: &str) -> html5ever::rcdom::RcDom {
    parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut data.as_bytes())
        .unwrap()
}

pub fn print_dom(handle: &Handle) {
    serialize(&mut io::stdout(), handle, SerializeOpts::default()).unwrap();
}

fn is_icon(attr_value: &str) -> bool {
    ICON_VALUES.is_match(&attr_value.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_icon() {
        assert_eq!(is_icon("icon"), true);
        assert_eq!(is_icon("Shortcut Icon"), true);
        assert_eq!(is_icon("ICON"), true);
        assert_eq!(is_icon("mask-icon"), true);
        assert_eq!(is_icon("fluid-icon"), true);
        assert_eq!(is_icon("stylesheet"), false);
        assert_eq!(is_icon(""), false);
    }

    #[test]
    fn test_has_protocol() {
        assert_eq!(has_protocol("mailto:somebody@somewhere.com?subject=hello"), true);
        assert_eq!(has_protocol("tel:5551234567"), true);
        assert_eq!(has_protocol("ftp:user:password@some-ftp-server.com"), true);
        assert_eq!(has_protocol("javascript:void(0)"), true);
        assert_eq!(has_protocol("http://news.ycombinator.com"), true);
        assert_eq!(has_protocol("https://github.com"), true);
        assert_eq!(has_protocol("//some-hostname.com/some-file.html"), false);
        assert_eq!(has_protocol("some-hostname.com/some-file.html"), false);
        assert_eq!(has_protocol("/some-file.html"), false);
        assert_eq!(has_protocol(""), false);
        assert_eq!(has_protocol("MAILTO:somebody@somewhere.com?subject=hello"), true);
    }

    #[test]
    fn test_get_parent_node_name() {
        let html = "<!doctype html><html><HEAD></HEAD><body><div><P></P></div></body></html>";
        let dom = html_to_dom(&html);
        let mut count = 0;

        fn test_walk(node: &Handle, i: &mut i8) {
            *i += 1;

            match &node.data {
                NodeData::Document => {
                    for child in node.children.borrow().iter() {
                        test_walk(child, &mut *i);
                    }
                }
                NodeData::Doctype { .. } => (),
                NodeData::Text { .. } => (),
                NodeData::Comment { .. } => (),
                NodeData::Element { ref name, attrs: _, .. } => {
                    let node_name = name.local.as_ref().to_string();
                    let parent_node_name = get_parent_node_name(node);
                    if node_name == "head" || node_name == "body" {
                        assert_eq!(parent_node_name, "html");
                    } else if node_name == "div" {
                        assert_eq!(parent_node_name, "body");
                    } else if node_name == "p" {
                        assert_eq!(parent_node_name, "div");
                    }

                    println!("{}", node_name);

                    for child in node.children.borrow().iter() {
                        test_walk(child, &mut *i);
                    }
                }
                NodeData::ProcessingInstruction { .. } => unreachable!()
            };
        }

        test_walk(&dom.document, &mut count);

        assert_eq!(count, 7);
    }

    #[test]
    fn test_walk_and_embed_assets() {
        let html = "<div><P></P></div>";
        let dom = html_to_dom(&html);
        let url = "http://localhost";

        walk_and_embed_assets(&url, &dom.document, true, true, "", true, true);

        let mut buf: Vec<u8> = Vec::new();
        serialize(&mut buf, &dom.document, SerializeOpts::default()).unwrap();

        assert_eq!(
            buf.iter().map(|&c| c as char).collect::<String>(),
            "<html><head></head><body><div><p></p></div></body></html>"
        );
    }

    #[test]
    fn test_walk_and_embed_assets_iframe() {
        let html = "<div><P></P><iframe src=\"\"></iframe></div>";
        let dom = html_to_dom(&html);
        let url = "http://localhost";

        walk_and_embed_assets(&url, &dom.document, true, true, "", true, true);

        let mut buf: Vec<u8> = Vec::new();
        serialize(&mut buf, &dom.document, SerializeOpts::default()).unwrap();

        assert_eq!(
            buf.iter().map(|&c| c as char).collect::<String>(),
            "<html><head></head><body><div><p></p><iframe src=\"\"></iframe></div></body></html>"
        );
    }

    #[test]
    fn test_walk_and_embed_assets_img() {
        let html = "<div><img src=\"http://localhost/assets/mono_lisa.png\" /></div>";
        let dom = html_to_dom(&html);
        let url = "http://localhost";

        walk_and_embed_assets(&url, &dom.document, true, true, "", true, true);

        let mut buf: Vec<u8> = Vec::new();
        serialize(&mut buf, &dom.document, SerializeOpts::default()).unwrap();

        assert_eq!(
            buf.iter().map(|&c| c as char).collect::<String>(),
            "<html><head></head><body><div>\
             <img src=\"data:image/png;base64,\
             iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0\
             lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=\">\
             </div></body></html>"
        );
    }

    #[test]
    fn test_walk_and_embed_assets_js() {
        let html = "<div><script src=\"http://localhost/assets/some.js\"></script>\
                    <script>alert(1)</script></div>";
        let dom = html_to_dom(&html);
        let url = "http://localhost";

        walk_and_embed_assets(&url, &dom.document, true, true, "", true, true);

        let mut buf: Vec<u8> = Vec::new();
        serialize(&mut buf, &dom.document, SerializeOpts::default()).unwrap();

        assert_eq!(
            buf.iter().map(|&c| c as char).collect::<String>(),
            "<html><head></head><body><div><script src=\"\"></script>\
             <script></script></div></body></html>"
        );
    }
}
